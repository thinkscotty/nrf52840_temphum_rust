#![no_std]
#![no_main]

//! Phase B — on-board hardware bring-up & verification.
//!
//! Replaces the Phase-A blinky. A repeating diagnostic, logged over USB-CDC
//! (read it with `scripts/read-serial.sh`), that answers the open questions in
//! HARDWARE.md §9 with measurements taken on the real board:
//!
//!   1. P0.13 VCC-rail polarity  — which level powers the gated VCC rail?
//!   2. I²C bus scan (P0.17/P0.20) — does the AHT20 answer at 0x38?
//!   3. Battery ADC (AIN7/P0.31, gated by Q1 on P0.22) — does Vbat/2 read right?
//!
//! Tests 1 and 2 are *fused*: we sweep P0.13 HIGH then LOW and scan I²C in each
//! state. Whichever level makes 0x38 appear **is** the VCC-ON level, so the scan
//! resolves the polarity question for free. This only works because the AHT20
//! breakout's pull-ups sit on the gated VCC rail (HARDWARE.md §5.2), so the bus
//! is genuinely dead when VCC is off — which is why we keep the TWIM internal
//! pull-ups DISABLED here.
//!
//! Test 4 (watchdog reset) lives behind the `wdt-test` cargo feature because it
//! deliberately resets the chip; isolating it keeps the readable diagnostic loop
//! from being interrupted. Run it with:
//!     cargo run --release --features wdt-test
//! It resets once, then prints a ✓ on the next boot (detected via RESETREAS.DOG,
//! so it does not boot-loop). Return to the diagnostic with `cargo run --release`.

use embassy_executor::Spawner;
use embassy_nrf::config::{Config, HfclkSource};
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::usb::vbus_detect::HardwareVbusDetect;
use embassy_nrf::usb::Driver;
use embassy_nrf::{bind_interrupts, peripherals, usb};
use embassy_time::Timer;
use embassy_usb_logger::ReceiverHandler;
use panic_halt as _;

// SAADC + the scan helpers are only used by the full diagnostic.
#[cfg(not(any(feature = "wdt-test", feature = "i2c-probe")))]
use embassy_nrf::saadc::{self, ChannelConfig, Oversample, Resolution, Saadc, Time};
// TWIM + timeout are shared by the full diagnostic and the i2c-probe build.
#[cfg(not(feature = "wdt-test"))]
use embassy_nrf::twim::{self, Twim};
#[cfg(not(feature = "wdt-test"))]
use embassy_time::{with_timeout, Duration};

#[cfg(feature = "wdt-test")]
use embassy_nrf::wdt::{self, Watchdog};

// USBD drives the device stack; CLOCK_POWER feeds VBUS detection. TWISPI0 and
// SAADC back the I²C scan and battery ADC (bound in both builds — harmless when
// the wdt-test build never touches those peripherals). Handler paths are fully
// qualified so the macro needs no `use` that a given build might gate away.
bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<peripherals::USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
    TWISPI0 => embassy_nrf::twim::InterruptHandler<peripherals::TWISPI0>;
    SAADC => embassy_nrf::saadc::InterruptHandler;
});

/// Watches the USB-CDC input for the literal `dfu` and reboots straight into the
/// UF2 bootloader, so flashing never needs the reset-button double-tap. Triggered
/// by `scripts/enter-dfu.sh` (and automatically by `scripts/flash-uf2.sh`).
struct DfuHandler;

impl embassy_usb_logger::ReceiverHandler for DfuHandler {
    async fn handle_data(&self, data: &[u8]) {
        if data.windows(3).any(|w| w == b"dfu") {
            // The Adafruit nRF52 bootloader enters UF2 mass-storage DFU when it
            // sees GPREGRET == 0x57 (DFU_MAGIC_UF2_RESET) after a system reset.
            embassy_nrf::pac::POWER.gpregret().write(|w| w.set_gpregret(0x57));
            cortex_m::peripheral::SCB::sys_reset();
        }
    }
    fn new() -> Self {
        Self
    }
}

/// Owns the USB device stack + CDC ACM class + `log` bridge. Never returns.
#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, HardwareVbusDetect>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver, DfuHandler);
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // USBD needs the 64 MHz crystal (HFXO) to enumerate; the internal RC is too
    // imprecise. Works here only because the S140 SoftDevice is dormant — see
    // the Phase-D caveat in the usb-cdc-logging notes.
    let mut config = Config::default();
    config.hfclk_source = HfclkSource::ExternalXtal;
    let p = embassy_nrf::init(config);

    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));
    spawner.spawn(logger_task(driver).unwrap());

    // Let the host enumerate before the first report. Early lines are buffered
    // (~1 KiB) and drain on connect, and every loop repeats, so nothing is lost.
    Timer::after_millis(1500).await;

    // User LED on P0.15, active-low. Used here as a per-cycle heartbeat.
    let mut led = Output::new(p.P0_15, Level::High, OutputDrive::Standard);

    // ===================== Test 4: watchdog reset =====================
    #[cfg(feature = "wdt-test")]
    {
        use embassy_nrf::pac;

        // RESETREAS.DOG is sticky and survives a watchdog reset, so it reports
        // whether the reset we may have just taken was the watchdog's. Read it,
        // then clear it (write-1-to-clear) so the verdict reflects only this run.
        let was_dog = pac::POWER.resetreas().read().dog();
        pac::POWER.resetreas().write(|w| w.set_dog(true));

        if was_dog {
            log::info!("✓ WDT RESET CONFIRMED — last reset was the watchdog (RESETREAS.DOG=1).");
            log::info!("  The watchdog fires correctly. Reflash the diagnostic: cargo run --release");
            loop {
                led.set_low();
                Timer::after_millis(40).await;
                led.set_high();
                Timer::after_millis(1960).await;
            }
        }

        log::info!("WDT smoke test: arming a 5 s watchdog, petting 3× (~1.5 s apart), then STOPPING.");
        let mut wdt_config = wdt::Config::default();
        wdt_config.timeout_ticks = 5 * 32_768; // 5 s @ 32.768 kHz

        let (_wdt, handles) = match Watchdog::try_new::<peripherals::WDT, 1>(p.WDT, wdt_config) {
            Ok(v) => v,
            Err(_) => {
                log::error!("WDT already running with a different config — power-cycle to clear it.");
                loop {
                    led.set_low();
                    Timer::after_millis(40).await;
                    led.set_high();
                    Timer::after_millis(460).await;
                }
            }
        };
        let [mut handle] = handles;

        for i in 1..=3 {
            handle.pet();
            log::info!("  pet {}/3 (watchdog reloaded)", i);
            led.set_low();
            Timer::after_millis(40).await;
            led.set_high();
            Timer::after_millis(1460).await;
        }

        log::warn!("  NO LONGER PETTING — expect a reset in ~5 s; USB will drop and re-enumerate.");
        log::warn!("  On the next boot this build prints the ✓ confirmation.");
        loop {
            Timer::after_millis(1000).await;
            log::info!("  ...waiting for watchdog reset...");
        }
    }

    // ============== I²C probe: clean power-cycle each window ==============
    #[cfg(feature = "i2c-probe")]
    {
        // Each window does a FULL power-cycle of the AHT20 to prove the gating
        // can give a clean power-on reset:
        //   power-up:   P0.13 HIGH → make TWIM → settle → read 0x38 ×N
        //   power-down: drop TWIM → drive SDA+SCL LOW (kills I/O back-power so the
        //               sensor can't stay parasitically alive) → P0.13 LOW → bleed
        // We hold the pins as owned handles and `reborrow()` them between the TWIM
        // and plain GPIO each cycle. If 0x38 ACKs every cycle, firmware fixes it; if
        // it still wedges, the rail isn't bleeding to 0 and we add a discharge MOSFET.
        let mut vcc = Output::new(p.P0_13, Level::Low, OutputDrive::Standard);
        let mut twispi0 = p.TWISPI0;
        let mut sda = p.P0_17;
        let mut scl = p.P0_20;

        log::info!("");
        log::info!("########## I²C PROBE — clean power-cycle ##########");
        log::info!("Full AHT20 power-cycle each window. Analyzer: SDA=P0.17, SCL=P0.20, GND.");

        let mut cycle: u32 = 0;
        loop {
            cycle = cycle.wrapping_add(1);

            // --- power UP ---
            vcc.set_high();
            let (mut ok, mut nack, mut timeout, mut other) = (0u16, 0u16, 0u16, 0u16);
            {
                let mut twim_buf = [0u8; 16];
                let mut i2c = Twim::new(
                    twispi0.reborrow(),
                    Irqs,
                    sda.reborrow(),
                    scl.reborrow(),
                    twim::Config::default(),
                    &mut twim_buf,
                );
                Timer::after_millis(100).await; // AHT20 power-on settle (>40 ms)
                for _ in 0..20 {
                    let mut b = [0u8; 1];
                    match with_timeout(Duration::from_millis(50), i2c.read(0x38, &mut b)).await {
                        Ok(Ok(())) => ok += 1,
                        Ok(Err(twim::Error::AddressNack)) => nack += 1,
                        Ok(Err(_)) => other += 1,
                        Err(_) => timeout += 1,
                    }
                    Timer::after_millis(50).await;
                }
            } // TWIM dropped → SDA/SCL released

            log::info!(
                "cycle {} (powered): 0x38 ×20 -> ok={} nack={} timeout={} other={}",
                cycle, ok, nack, timeout, other
            );

            // --- power DOWN, cleanly ---
            {
                // Pin SDA+SCL low so the AHT20 can't back-power through its I/O pins.
                let _sda_lo = Output::new(sda.reborrow(), Level::Low, OutputDrive::Standard);
                let _scl_lo = Output::new(scl.reborrow(), Level::Low, OutputDrive::Standard);
                vcc.set_low(); // rail off (and, if P0.13 drives the rail, actively sinks it)
                Timer::after_millis(500).await; // bleed to ~0 for a clean POR next cycle
            } // GPIO-low released

            led.set_low();
            Timer::after_millis(40).await;
            led.set_high();
        }
    }

    // ============== Tests 1–3: VCC polarity, I²C scan, battery ADC ==============
    #[cfg(not(any(feature = "wdt-test", feature = "i2c-probe")))]
    {
        // VCC-rail control. Boot LOW (we don't yet know which level is "off");
        // test 1 discovers it. P0.22 = Q1 gate: boot LOW so the battery divider
        // is open (Q1 off) — never leave it floating/high.
        let mut vcc = Output::new(p.P0_13, Level::Low, OutputDrive::Standard);
        let mut batt_en = Output::new(p.P0_22, Level::Low, OutputDrive::Standard);

        // I²C on P0.17 (SDA) / P0.20 (SCL). Internal pull-ups OFF on purpose
        // (see module docs). `tx_ram_buffer` must outlive the Twim; scans only
        // read, so a small RAM buffer is plenty.
        let mut twim_buf = [0u8; 16];
        let i2c_config = twim::Config::default();
        let mut i2c = Twim::new(p.TWISPI0, Irqs, p.P0_17, p.P0_20, i2c_config, &mut twim_buf);

        // SAADC on AIN7 (P0.31): 12-bit, 4× oversample. single_ended() already
        // defaults to gain 1/6 + internal 0.6 V ref → full-scale 3.6 V (Vbat/2
        // ≤ 2.1 V fits). 40 µs acquisition for the high-impedance 100k divider.
        let mut adc_config = saadc::Config::default();
        adc_config.resolution = Resolution::_12BIT;
        adc_config.oversample = Oversample::OVER4X;
        let mut ch = ChannelConfig::single_ended(p.P0_31);
        ch.time = Time::_40US;
        let mut adc = Saadc::new(p.SAADC, Irqs, adc_config, [ch]);
        adc.calibrate().await;

        log::info!("");
        log::info!("########## Phase B hardware diagnostic (USB-CDC) ##########");
        log::info!("Optional cross-checks with a multimeter:");
        log::info!("  • during the P0.13 sweep, probe the VCC pin (expect ~3.3 V in the ON state)");
        log::info!("  • for test 3, probe B+ and compare to the reported Vbat");
        log::info!("Looping every ~10 s. Ctrl-C to stop reading; the board keeps running.");

        let mut cycle: u32 = 0;
        loop {
            cycle = cycle.wrapping_add(1);
            log::info!("");
            log::info!("================ cycle {} ================", cycle);

            // ---- Tests 1 + 2: VCC-rail polarity + I²C scan ----
            log::info!("[1+2] VCC-rail polarity & I²C scan (SDA=P0.17, SCL=P0.20)");

            vcc.set_high();
            log::info!("  P0.13 = HIGH  (measure VCC pin now; held ~3 s)");
            Timer::after_millis(150).await; // > AHT20 ~40 ms power-on, if this powers it
            let hi = i2c_scan(&mut i2c).await;
            report_scan("HIGH", &hi);
            Timer::after_millis(3000).await;

            vcc.set_low();
            log::info!("  P0.13 = LOW   (measure VCC pin now; held ~3 s)");
            Timer::after_millis(150).await;
            let lo = i2c_scan(&mut i2c).await;
            report_scan("LOW", &lo);
            Timer::after_millis(3000).await;

            match (hi.found_aht, lo.found_aht) {
                (true, false) => {
                    log::info!("  => VCC rail is ON when P0.13 = HIGH  (VCC_RAIL_ON_LEVEL = High)")
                }
                (false, true) => {
                    log::info!("  => VCC rail is ON when P0.13 = LOW   (VCC_RAIL_ON_LEVEL = Low)")
                }
                (true, true) => log::warn!(
                    "  => 0x38 answered in BOTH states — pull-ups likely NOT on the gated rail \
                     (parasitic power via I²C lines). Recheck HARDWARE.md §5.2."
                ),
                (false, false) => log::warn!(
                    "  => 0x38 answered in NEITHER state — AHT20 not responding. Check wiring, \
                     pull-ups, and that VCC actually switches."
                ),
            }
            vcc.set_low(); // park; the confirmed off-level gets baked in later

            // ---- Test 3: battery ADC via Q1 gate (P0.22), AIN7 = P0.31 ----
            log::info!("[3] Battery ADC (AIN7/P0.31), gated by Q1 on P0.22");
            let mut buf = [0i16; 1];

            batt_en.set_high(); // Q1 ON → divider bottom to GND → node ≈ Vbat/2
            Timer::after_millis(2).await; // settle (divider RC ~50 µs)
            adc.sample(&mut buf).await;
            let raw_on = buf[0].max(0) as i32;
            let v_pin_on = raw_on * 3600 / 4096; // mV at the ADC pin
            let v_bat_on = v_pin_on * 2; // ×2 for the 100k/100k divider
            log::info!(
                "  gate ON : raw={:4}  Vpin={}.{:03} V  Vbat≈{}.{:03} V",
                raw_on,
                v_pin_on / 1000,
                v_pin_on % 1000,
                v_bat_on / 1000,
                v_bat_on % 1000
            );

            batt_en.set_low(); // Q1 OFF → divider bottom open
            Timer::after_millis(2).await;
            adc.sample(&mut buf).await;
            let raw_off = buf[0].max(0) as i32;
            let v_pin_off = raw_off * 3600 / 4096;
            log::info!(
                "  gate OFF: raw={:4}  Vpin={}.{:03} V  (near full-scale ⇒ bottom open, Q1 gating works)",
                raw_off,
                v_pin_off / 1000,
                v_pin_off % 1000
            );

            // heartbeat
            led.set_low();
            Timer::after_millis(60).await;
            led.set_high();

            Timer::after_millis(3000).await;
        }
    }
}

/// Result of one I²C address sweep.
#[cfg(not(any(feature = "wdt-test", feature = "i2c-probe")))]
struct ScanResult {
    /// Number of addresses that ACKed.
    count: u8,
    /// Whether the AHT20's address (0x38) answered.
    found_aht: bool,
    /// Bus appeared inactive (lines never released high → no powered pull-ups).
    bus_dead: bool,
}

/// Sweep 7-bit addresses 0x08..=0x77 with a 1-byte read; an ACK marks a device.
/// Each read is bounded by a timeout so a stuck bus (no pull-ups) can't hang us;
/// four consecutive timeouts with nothing found means the bus is dead and we bail.
#[cfg(not(any(feature = "wdt-test", feature = "i2c-probe")))]
async fn i2c_scan(i2c: &mut Twim<'_>) -> ScanResult {
    let mut count = 0u8;
    let mut found_aht = false;
    let mut consec_timeouts = 0u8;

    for addr in 0x08u8..=0x77 {
        let mut b = [0u8; 1];
        match with_timeout(Duration::from_millis(40), i2c.read(addr, &mut b)).await {
            Ok(Ok(())) => {
                count += 1;
                if addr == 0x38 {
                    found_aht = true;
                }
                log::info!("    responder @ 0x{:02X}", addr);
                consec_timeouts = 0;
            }
            // Clean "nobody home" — the common case for empty addresses.
            Ok(Err(twim::Error::AddressNack)) => consec_timeouts = 0,
            // Address ACKed but the data phase complained — still a live device.
            Ok(Err(e)) => {
                count += 1;
                if addr == 0x38 {
                    found_aht = true;
                }
                log::info!("    0x{:02X}: addr ACKed, data phase {:?}", addr, e);
                consec_timeouts = 0;
            }
            Err(_timeout) => {
                consec_timeouts += 1;
                if count == 0 && consec_timeouts >= 4 {
                    return ScanResult {
                        count,
                        found_aht,
                        bus_dead: true,
                    };
                }
            }
        }
    }

    ScanResult {
        count,
        found_aht,
        bus_dead: false,
    }
}

#[cfg(not(any(feature = "wdt-test", feature = "i2c-probe")))]
fn report_scan(state: &str, r: &ScanResult) {
    if r.bus_dead {
        log::info!(
            "  P0.13={}: I²C bus inactive (lines not pulled up ⇒ rail OFF in this state)",
            state
        );
    } else {
        log::info!(
            "  P0.13={}: {} responder(s); AHT20(0x38) {}",
            state,
            r.count,
            if r.found_aht { "PRESENT" } else { "absent" }
        );
    }
}

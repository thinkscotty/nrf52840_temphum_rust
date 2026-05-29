#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_nrf::config::{Config, HfclkSource};
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::usb::vbus_detect::HardwareVbusDetect;
use embassy_nrf::usb::Driver;
use embassy_nrf::{bind_interrupts, peripherals, usb};
use embassy_time::Timer;
use panic_halt as _;

// USBD drives the device stack; CLOCK_POWER feeds VBUS detection (the nRF52840
// reports USB power state through the POWER peripheral, whose IRQ is CLOCK_POWER).
bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<peripherals::USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
});

/// Owns the USB device stack + CDC ACM class + `log` bridge. Never returns.
#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, HardwareVbusDetect>) {
    // 1 KiB pipe, Info level. The macro installs the global logger and runs
    // the USB device forever; lines logged before a host connects are buffered.
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // The nRF52840 USBD needs the accurate 64 MHz crystal (HFXO) to enumerate;
    // the internal RC oscillator is too imprecise. `init` starts and waits for
    // the crystal when we select ExternalXtal.
    //
    // This works because the S140 SoftDevice present in flash is DORMANT (never
    // enabled). When BLE is brought up in Phase D the SoftDevice will own both
    // the clock and USB power events — at that point switch to a SoftwareVbusDetect
    // fed by SoftDevice events, and request the HFCLK through the SoftDevice,
    // rather than driving the CLOCK/POWER registers directly as we do here.
    let mut config = Config::default();
    config.hfclk_source = HfclkSource::ExternalXtal;
    let p = embassy_nrf::init(config);

    // Bring up USB-CDC logging first so the heartbeat below is observable.
    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));
    // embassy-executor 0.10: the #[task] fn returns Result<SpawnToken, _>
    // (token allocation is the fallible step); Spawner::spawn takes the token.
    spawner.spawn(logger_task(driver).unwrap());

    // User LED on P0.15, active-low (pin LOW = LED ON). Verified on the Teyleten
    // clone (drives a red LED here). Distinctive double-blink + long pause so it
    // is unmistakably our firmware rather than a bootloader/charge pattern.
    let mut led = Output::new(p.P0_15, Level::High, OutputDrive::Standard);

    let mut tick: u32 = 0;
    loop {
        led.set_low();
        Timer::after_millis(80).await;
        led.set_high();
        Timer::after_millis(180).await;
        led.set_low();
        Timer::after_millis(80).await;
        led.set_high();
        Timer::after_millis(1500).await;

        tick = tick.wrapping_add(1);
        log::info!("tick {} — alive, USB-CDC logging up", tick);
    }
}

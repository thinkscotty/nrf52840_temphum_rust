//! AHT20 temperature/humidity driver — owns its power rail and bus pins.
//!
//! This is an in-tree driver (not a wrapper around `ahtx0`) because the AHT20 on
//! this board is **power-gated on the P0.13 VCC rail and back-powers through its
//! I²C lines** if naively cut. A generic I²C-bus driver can't see the rail or the
//! SDA/SCL pins, so it can't perform the clean power-cycle the part needs. The
//! TLV protocol is tiny (a handful of commands), so we own it end-to-end.
//!
//! ## The clean power-cycle (bench-verified, Phase B)
//!
//! A gated AHT20 stays parasitically alive through its SDA/SCL ESD clamps and
//! cap charge, so simply dropping VCC never gives it a power-on reset and it
//! wedges (ACKs once, then times out forever). The fix, proven on hardware:
//!
//!   drive SDA(P0.17)+SCL(P0.20) LOW → P0.13 LOW → bleed ~500 ms → P0.13 HIGH →
//!   re-init TWIM → wait ~100 ms
//!
//! [`Aht20::measure`] runs this every call, so each reading starts from a
//! guaranteed POR regardless of how the previous cycle ended, and leaves the
//! rail OFF afterwards (the AHT20 is unpowered between cycles — good for sleep).
//!
//! Every I²C transaction is bounded by [`I2C_TIMEOUT`]; a stuck bus can never
//! hang the caller. See [`super::sensors`] for how failures are folded into the
//! per-cycle `Sample` without panicking.

use embassy_nrf::gpio::{AnyPin, Level, Output, OutputDrive, Pin};
use embassy_nrf::interrupt::typelevel::Binding;
use embassy_nrf::peripherals::TWISPI0;
use embassy_nrf::twim::{self, Instance, InterruptHandler, Twim};
use embassy_nrf::Peri;
use embassy_time::{with_timeout, Duration, Timer};

/// AHT20 I²C address (7-bit). Pull-ups (~10 kΩ) live on the breakout's gated rail.
const ADDR: u8 = 0x38;

/// Upper bound on any single I²C transaction. The bus can hang on a hardware
/// fault (no pull-ups, shorted line); we never want to block a cycle on it.
const I2C_TIMEOUT: Duration = Duration::from_millis(500);

/// Status byte bits returned by the AHT20.
const STATUS_BUSY: u8 = 0x80; // 1 = measurement in progress
const STATUS_CAL: u8 = 0x08; // 1 = factory-calibrated and ready

// Command opcodes (AHT20 datasheet §5).
const CMD_SOFT_RESET: u8 = 0xBA;
const CMD_INIT: [u8; 3] = [0xBE, 0x08, 0x00];
const CMD_MEASURE: [u8; 3] = [0xAC, 0x33, 0x00];

/// A successful reading, already converted to fixed-point engineering units.
#[derive(Clone, Copy, Debug)]
pub struct Reading {
    /// Temperature in °C × 100 (e.g. 2137 = 21.37 °C).
    pub temperature_c_centi: i16,
    /// Relative humidity in % × 100 (e.g. 4850 = 48.50 %RH).
    pub humidity_rh_centi: u16,
}

/// Why a measurement failed. Every variant is recoverable — the caller logs it
/// and skips the field for this cycle; the next cycle power-cycles afresh.
//
// The `Bus` payload is surfaced only through the derived `Debug` (in the
// caller's `log::warn!`), which the dead-code lint doesn't count as a read.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub enum Error {
    /// An I²C transaction exceeded [`I2C_TIMEOUT`] — wedged bus or absent sensor.
    Timeout,
    /// A TWIM transfer failed (NACK, overrun, …).
    Bus(twim::Error),
    /// The measurement still read busy after the conversion-wait budget.
    StillBusy,
    /// Data CRC8 mismatch — corrupt transfer.
    Crc,
    /// Calibration bit never came up even after sending the init command.
    Uncalibrated,
}

impl From<twim::Error> for Error {
    fn from(e: twim::Error) -> Self {
        Error::Bus(e)
    }
}

/// Owns the AHT20's VCC rail (P0.13) and the shared I²C pins (P0.17/P0.20) plus
/// the TWISPI0 instance, so it can flip them between TWIM and plain-GPIO roles
/// during the power-cycle. `IRQ` is the `bind_interrupts!` token (zero-sized,
/// `Copy`) that authorizes building a [`Twim`] each cycle.
pub struct Aht20<'d, IRQ> {
    vcc: Output<'d>,
    twispi0: Peri<'d, TWISPI0>,
    sda: Peri<'d, AnyPin>,
    scl: Peri<'d, AnyPin>,
    irq: IRQ,
}

impl<'d, IRQ> Aht20<'d, IRQ>
where
    IRQ: Binding<<TWISPI0 as Instance>::Interrupt, InterruptHandler<TWISPI0>> + Copy + 'd,
{
    /// Take ownership of the rail pin, TWISPI0, and the SDA/SCL pins. The rail
    /// starts OFF (P0.13 LOW). Pins are accepted concretely and type-erased
    /// internally so callers just pass `p.P0_13`, `p.P0_17`, `p.P0_20`.
    pub fn new(
        vcc: Peri<'d, impl Pin>,
        twispi0: Peri<'d, TWISPI0>,
        sda: Peri<'d, impl Pin>,
        scl: Peri<'d, impl Pin>,
        irq: IRQ,
    ) -> Self {
        let sda: Peri<'d, AnyPin> = sda.into();
        let scl: Peri<'d, AnyPin> = scl.into();
        Self {
            vcc: Output::new(vcc, Level::Low, OutputDrive::Standard),
            twispi0,
            sda,
            scl,
            irq,
        }
    }

    /// Power-cycle, initialize, trigger one measurement, and return the reading.
    /// Always leaves the AHT20 powered down on exit (success or failure).
    pub async fn measure(&mut self) -> Result<Reading, Error> {
        self.power_off().await; // known-good starting state: lines low, rail bled
        self.vcc.set_high();

        let result = self.measure_powered().await;

        self.power_off().await;
        result
    }

    /// The bus-active portion of a cycle: rail is already HIGH on entry. Builds a
    /// TWIM over the reborrowed pins, talks to the sensor, and drops the TWIM
    /// before returning so [`power_off`](Self::power_off) can reclaim the pins.
    async fn measure_powered(&mut self) -> Result<Reading, Error> {
        let mut tx_buf = [0u8; 8]; // TWIM DMA scratch; commands are ≤3 bytes
        let mut i2c = Twim::new(
            self.twispi0.reborrow(),
            self.irq,
            self.sda.reborrow(),
            self.scl.reborrow(),
            twim::Config::default(),
            &mut tx_buf,
        );

        // AHT20 needs ≥40 ms after power-on before it accepts commands.
        Timer::after_millis(100).await;

        // Soft-reset for a clean register state, then ≥20 ms per datasheet.
        io(i2c.write(ADDR, &[CMD_SOFT_RESET])).await?;
        Timer::after_millis(20).await;

        // Ensure the calibration/loaded bit is set; send init once if not.
        let mut status = [0u8; 1];
        io(i2c.read(ADDR, &mut status)).await?;
        if status[0] & STATUS_CAL == 0 {
            io(i2c.write(ADDR, &CMD_INIT)).await?;
            Timer::after_millis(10).await;
            io(i2c.read(ADDR, &mut status)).await?;
            if status[0] & STATUS_CAL == 0 {
                return Err(Error::Uncalibrated);
            }
        }

        // Trigger a measurement; conversion takes ~80 ms.
        io(i2c.write(ADDR, &CMD_MEASURE)).await?;
        Timer::after_millis(80).await;

        // Poll the busy bit. Budget: up to ~10 extra reads at 10 ms spacing.
        let mut raw = [0u8; 7];
        let mut ready = false;
        for _ in 0..10 {
            io(i2c.read(ADDR, &mut raw)).await?;
            if raw[0] & STATUS_BUSY == 0 {
                ready = true;
                break;
            }
            Timer::after_millis(10).await;
        }
        if !ready {
            return Err(Error::StillBusy);
        }

        // raw = [status, hum[19:12], hum[11:4], hum[3:0]|temp[19:16], temp[15:8], temp[7:0], crc]
        if crc8(&raw[..6]) != raw[6] {
            return Err(Error::Crc);
        }

        Ok(convert(&raw))
    }

    /// Drive SDA/SCL LOW (kill I/O back-power), cut the rail, and let it bleed to
    /// ~0 V so the next [`measure`](Self::measure) starts from a true POR. Holds
    /// the pins low only for the bleed, then releases them.
    pub async fn power_off(&mut self) {
        {
            let _sda_lo = Output::new(self.sda.reborrow(), Level::Low, OutputDrive::Standard);
            let _scl_lo = Output::new(self.scl.reborrow(), Level::Low, OutputDrive::Standard);
            self.vcc.set_low();
            Timer::after_millis(500).await; // bleed for a clean POR next cycle
        } // GPIO-low drivers dropped → pins released
    }
}

/// Run one I²C transaction under the shared timeout, flattening the nested
/// `Result<Result<_, twim::Error>, TimeoutError>` into our [`Error`].
async fn io<F>(fut: F) -> Result<(), Error>
where
    F: core::future::Future<Output = Result<(), twim::Error>>,
{
    match with_timeout(I2C_TIMEOUT, fut).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(Error::Bus(e)),
        Err(_) => Err(Error::Timeout),
    }
}

/// Decode the 6 data bytes into fixed-point °C×100 and %RH×100.
///
/// Both fields are 20-bit. Per the datasheet:
///   RH%  = raw_h / 2^20 * 100
///   T°C  = raw_t / 2^20 * 200 − 50
/// We scale by 100 for centi-units and compute in i64/u64 to avoid overflow
/// (raw maxes at 2^20, so the products fit comfortably).
fn convert(raw: &[u8; 7]) -> Reading {
    let raw_h = ((raw[1] as u32) << 12) | ((raw[2] as u32) << 4) | ((raw[3] as u32) >> 4);
    let raw_t = (((raw[3] as u32) & 0x0F) << 16) | ((raw[4] as u32) << 8) | (raw[5] as u32);

    // humidity ×100, rounds to nearest: (raw_h * 10000) >> 20
    let humidity_rh_centi = ((raw_h as u64 * 10_000) >> 20) as u16;

    // temperature ×100: (raw_t * 20000) >> 20 − 5000, kept signed for sub-zero.
    let temp_centi = ((raw_t as i64 * 20_000) >> 20) - 5_000;
    let temperature_c_centi = temp_centi as i16;

    Reading {
        temperature_c_centi,
        humidity_rh_centi,
    }
}

/// CRC-8 used by the AHT20: polynomial 0x31 (x⁸+x⁵+x⁴+1), init 0xFF, MSB-first.
fn crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0xFF;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ 0x31;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

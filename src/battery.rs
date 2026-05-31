//! Battery-voltage reading via the external 100k/100k divider, current-gated by
//! a 2N7000 (Q1) so the divider draws nothing between samples.
//!
//! The divider's bottom leg is switched to GND by Q1 (gate = P0.22). With the
//! gate HIGH the node sits at ≈Vbat/2 and the SAADC reads it on AIN7 (P0.31);
//! with the gate LOW the bottom opens and no current flows (HARDWARE.md §4).
//!
//! Bench-confirmed (Phase B): the ideal divider math below matched a multimeter
//! to within ~10 mV at 4.11 V, so v1 ships **without** per-device calibration.
//! Two-point calibration is deferred to Phase G; the constants here are the only
//! thing that would change.

use embassy_nrf::gpio::{Level, Output, OutputDrive, Pin};
use embassy_nrf::interrupt::typelevel::{Binding, SAADC};
use embassy_nrf::peripherals;
use embassy_nrf::saadc::{
    ChannelConfig, Config, Input, InterruptHandler, Oversample, Resolution, Saadc, Time,
};
use embassy_nrf::Peri;
use embassy_time::Timer;

/// SAADC reference voltage (internal 0.6 V) × inverse gain (1/6) → full-scale
/// input span in millivolts: 0.6 V × 6 = 3.6 V = 3600 mV.
const ADC_FULLSCALE_MV: i32 = 3600;
/// 12-bit conversion → 4096 codes across the full-scale span.
const ADC_COUNTS: i32 = 4096;
/// Divider ratio: equal 100k legs halve Vbat, so multiply the pin voltage by 2.
const DIVIDER_NUM: i32 = 2;

/// Settling delay after enabling Q1 before sampling. The 100k/100k divider into
/// the SAADC sample cap is high-impedance; a couple ms swamps the RC (~tens of
/// µs) with margin. Cheap relative to the 300 s cycle.
const SETTLE_MS: u64 = 2;

/// Plausibility window for a LiPo reading. Outside this, the divider is open
/// (gate stuck off → near full-scale) or something is wired wrong; we report it
/// rather than feed Home Assistant a bogus voltage.
const VALID_MIN_MV: u16 = 2000;
const VALID_MAX_MV: u16 = 5000;

/// Battery read failed.
//
// The `OutOfRange` payload is surfaced only through the derived `Debug` (in the
// caller's `log::warn!`), which the dead-code lint doesn't count as a read.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub enum Error {
    /// Computed voltage fell outside [`VALID_MIN_MV`]..=[`VALID_MAX_MV`] — most
    /// likely the divider bottom never closed (Q1 gating fault). Carries the
    /// out-of-range value for logging.
    OutOfRange(u16),
}

/// Owns the SAADC (with AIN7 configured) and the Q1 gate pin (P0.22).
pub struct Battery<'d> {
    adc: Saadc<'d, 1>,
    gate: Output<'d>,
}

impl<'d> Battery<'d> {
    /// Configure the SAADC for the high-impedance divider and calibrate it once.
    /// `ain` is the analog input pin (P0.31 / AIN7); `gate` drives Q1 (P0.22),
    /// and starts LOW so the divider is open until the first read.
    pub async fn new<IRQ>(
        saadc: Peri<'d, peripherals::SAADC>,
        irq: IRQ,
        ain: impl Input + 'd,
        gate: Peri<'d, impl Pin>,
    ) -> Self
    where
        IRQ: Binding<SAADC, InterruptHandler> + 'd,
    {
        // 12-bit, 4× hardware oversample to average out divider/ADC noise.
        let mut config = Config::default();
        config.resolution = Resolution::_12BIT;
        config.oversample = Oversample::OVER4X;

        // single_ended() defaults to gain 1/6 + internal 0.6 V ref → 3.6 V span.
        // 40 µs acquisition is the datasheet guidance for high-impedance sources.
        let mut ch = ChannelConfig::single_ended(ain);
        ch.time = Time::_40US;

        let adc = Saadc::new(saadc, irq, config, [ch]);
        adc.calibrate().await;

        Self {
            adc,
            gate: Output::new(gate, Level::Low, OutputDrive::Standard),
        }
    }

    /// Gate Q1 on, sample Vbat, gate off. Returns the battery voltage in mV.
    pub async fn read_mv(&mut self) -> Result<u16, Error> {
        self.gate.set_high(); // close divider bottom → node ≈ Vbat/2
        Timer::after_millis(SETTLE_MS).await;

        let mut buf = [0i16; 1];
        self.adc.sample(&mut buf).await;

        self.gate.set_low(); // open divider → zero quiescent draw

        let raw = buf[0].max(0) as i32;
        let v_pin = raw * ADC_FULLSCALE_MV / ADC_COUNTS; // mV at the ADC pin
        let v_bat = (v_pin * DIVIDER_NUM) as u16;

        if !(VALID_MIN_MV..=VALID_MAX_MV).contains(&v_bat) {
            return Err(Error::OutOfRange(v_bat));
        }
        Ok(v_bat)
    }
}

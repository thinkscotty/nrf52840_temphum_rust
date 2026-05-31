//! Sensor aggregator — one call, one [`Sample`], never panics.
//!
//! [`Sensors::sample_all`] reads the AHT20 and the battery in turn and folds any
//! failure into a *missing field* rather than propagating an error: each reading
//! is an [`Option`], `None` meaning "this sensor failed this cycle". That maps
//! directly onto BTHome v2's omit-the-field convention (Phase D) and keeps the
//! main loop branch-free — it always gets a `Sample` and always advertises.
//!
//! A disconnected AHT20 therefore yields `temperature: None, humidity: None`
//! with a valid `battery_mv`, logged once, no reset — which is exactly the Phase
//! C robustness deliverable.

use crate::aht20::Aht20;
use crate::battery::Battery;
use embassy_nrf::interrupt::typelevel::Binding;
use embassy_nrf::peripherals::TWISPI0;
use embassy_nrf::twim::{Instance, InterruptHandler};

/// One cycle's worth of readings. Missing fields (`None`) survived a sensor
/// failure and will be omitted from the BTHome packet.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    /// Monotonic per-cycle counter (wraps at 256) — becomes the BTHome packet ID.
    pub cycle_id: u8,
    /// Temperature in °C × 100, or `None` if the AHT20 read failed.
    pub temperature_c_centi: Option<i16>,
    /// Relative humidity in % × 100, or `None` if the AHT20 read failed.
    pub humidity_rh_centi: Option<u16>,
    /// Battery voltage in mV, or `None` if the ADC read failed / was implausible.
    pub battery_mv: Option<u16>,
}

/// Holds the individual sensor drivers and the rolling cycle counter.
pub struct Sensors<'d, IRQ> {
    aht20: Aht20<'d, IRQ>,
    battery: Battery<'d>,
    cycle_id: u8,
}

impl<'d, IRQ> Sensors<'d, IRQ>
where
    IRQ: Binding<<TWISPI0 as Instance>::Interrupt, InterruptHandler<TWISPI0>> + Copy + 'd,
{
    pub fn new(aht20: Aht20<'d, IRQ>, battery: Battery<'d>) -> Self {
        Self {
            aht20,
            battery,
            cycle_id: 0,
        }
    }

    /// Read every sensor once. Individual failures are logged and encoded as a
    /// missing field; this call itself is infallible.
    pub async fn sample_all(&mut self) -> Sample {
        self.cycle_id = self.cycle_id.wrapping_add(1);

        let (temperature_c_centi, humidity_rh_centi) = match self.aht20.measure().await {
            Ok(r) => (Some(r.temperature_c_centi), Some(r.humidity_rh_centi)),
            Err(e) => {
                log::warn!("cycle {}: AHT20 read failed: {:?}", self.cycle_id, e);
                (None, None)
            }
        };

        let battery_mv = match self.battery.read_mv().await {
            Ok(mv) => Some(mv),
            Err(e) => {
                log::warn!("cycle {}: battery read failed: {:?}", self.cycle_id, e);
                None
            }
        };

        Sample {
            cycle_id: self.cycle_id,
            temperature_c_centi,
            humidity_rh_centi,
            battery_mv,
        }
    }
}

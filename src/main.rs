#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    info!("nrf52840_aht20_rust — Phase A blinky");

    // Nice!Nano-style boards: user LED on P0.15, expected to be active-low
    // (pin LOW = LED ON). If the Teyleten clone has inverted polarity, the
    // brief-flash pattern below will appear inverted (long flash, short gap)
    // — useful diagnostic info on first boot.
    let mut led = Output::new(p.P0_15, Level::High, OutputDrive::Standard);

    let mut tick: u32 = 0;
    loop {
        led.set_low();
        Timer::after_millis(100).await;
        led.set_high();
        Timer::after_millis(900).await;
        tick = tick.wrapping_add(1);
        info!("tick {}", tick);
    }
}

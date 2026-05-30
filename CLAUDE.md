# NRF52840 Temperature Sensor Builkd

The goal of this project is to create a 'standard', stable sensor for my smart home. It will use an inexpensive NRF52840 Pro Micro board and an AHT20 temperature sensor to measure temperature and humidity and communicate readings periodically with Home Assistant using BLE (Bluetooth Low Energy) and the BTHome V2 Home Assistant integration. It will be powered by a single Lithium Polymer battery, either an 18650 or 21700 size. 

## Priorities
- Stability: above all else, code should run stably for up to a year or more without requiring resets or revisions
- Transmission strength: because the battery is large for such a sensor, we can afford repeated and strong transmissions
- Deep Sleep stability: the NRF52840 has famously low current draw in deep sleep and we will make use of sleep states to save battery. Extra care should be taken to ensure deep sleep is stable and wakes reliably

## Hardware
- NRF52840 Pro Micro microcontroller board. This is a budget tier clone of the Nice! Nano board. 'Teyleten Robot' is the vendor.
- AHT20 temperature and humidity sensor board (I2C)
- 21700 or 18650 LiPo battery (3.7V nominal)
- 2N7000 MOSFET (gate for voltage divider for battery savings)
- 100k 0.1% resistors for voltage divider
- Capacitors and resistors as required
- Strip-board perfboard
- 3D Printed Custom Case

## Tech Stack
- Rust
- Embassy-rs

## Flashing
- Flashed via USB-C

## Features
- Temperature sensing
- Humidity sensing
- Battery voltage sensing (via external voltage divider, not onboard, for accuracy) (gated by 2N7000 MOSFET)

## Development Environment 
- MacOS Tahoe 26.5
- ARM silicon Macbook Pro with 16GB RAM
- VSCode with Claude Code extension

## Deployment Environment 
- Home Assistant running on Raspberry Pi 5 on most up-to-date software
- ESPHome BT Proxies (IMPORTANT - Check BLE code compatibility)

## Notes
- Before we start creating code, we need to create a clear hardware build plan with pins
- Research the dev board pinout before deciding on pin numbers

## Bench-Confirmed Facts (Phase B — verified on hardware)
- **P0.13 VCC-rail polarity: HIGH = rail ON** (LOW = off). The rail bleeds to ~0 on its own when P0.13 goes low.
- **AHT20 must be power-cycled cleanly or it wedges.** Naive gating (just cut VCC) leaves it back-powered through SDA/SCL so it never POR-resets. Fix (verified): drive SDA(P0.17)+SCL(P0.20) LOW → set P0.13 LOW → bleed ~500ms → P0.13 HIGH → re-init TWIM → wait ~100ms. The Phase C `aht20.rs` driver must use this.
- **AHT20 I²C address 0x38**, pull-ups ~10 kΩ present on the breakout.
- **Battery divider is accurate enough to skip calibration for v1**: ideal `raw*3600/4096*2` matched a multimeter within ~10 mV (4.11 V). SAADC: AIN7/P0.31, gain 1/6, ref 0.6 V, 12-bit, 40 µs acq. P0.22 HIGH gates Q1 on; gate-OFF pegs near full-scale (divider bottom opens). Two-point cal (Phase G) is optional.
- **Layout gotcha:** SCL (P0.20) and the battery-enable pin (P0.22) are physically adjacent — a solder bridge shorted them and silently killed I²C. Keep that route clear.
- **Flashing is button-free**: firmware's `DfuHandler` reboots into UF2 DFU when it sees `dfu` on the USB-CDC input; `cargo run` triggers it automatically. Double-tap reset only needed for handler-less firmware.

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

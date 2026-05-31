# nrf52840_aht20_rust

Battery-powered BTHome v2 temperature/humidity sensor for Home Assistant.
nRF52840 Pro Micro + AHT20, written in async Rust on Embassy.

See [CLAUDE.md](CLAUDE.md) for project goals and [HARDWARE.md](HARDWARE.md) for
the hardware build spec. The firmware implementation plan lives in
`~/.claude/plans/enchanted-conjuring-dusk.md`.

## Status

**Phase C complete** — the production sensor drivers are written and verified on
hardware. The default build runs a sample loop that prints one reading set every
5 s over USB-CDC:

- `aht20.rs` — AHT20 driver with the bench-verified clean power-cycle baked in
  (no wedging across consecutive cycles), 500 ms I²C timeouts, CRC8 check
- `battery.rs` — SAADC + 2N7000-gated divider, ideal divider math (no cal for v1)
- `sensors.rs` — `sample_all()` aggregator; a failed sensor becomes a `None`
  field (logged, no panic) rather than aborting the cycle

Earlier phases: **Phase B** verified the hardware on the bench (AHT20 at `0x38`,
divider matches a multimeter, P0.13 VCC gating HIGH = on) — that diagnostic now
lives behind the `bringup` feature.

Next: **Phase D** — BLE + BTHome v2 broadcasting via nrf-softdevice (S140).

## Toolchain & flashing

- Rust toolchain pinned in `rust-toolchain.toml` (auto-installs the
  `thumbv7em-none-eabihf` target on first build).
- Flashed over **USB-C** via the board's UF2 bootloader (app slot `0x26000`,
  after the pre-flashed S140 SoftDevice). No debug probe needed.

```sh
cargo run --release          # build + flash + reboot, fully button-free
```

`cargo run` is wired to `scripts/flash-uf2.sh`, which converts the ELF to UF2
and copies it to the `NICENANO` volume. **Flashing needs no reset button**: the
firmware listens on its USB-CDC port for the string `dfu` and reboots itself
into the bootloader, and the flasher sends that automatically. (Double-tap reset
is only needed to install firmware that predates this handler.)

### Logging

Logs stream over **USB-CDC** (`log` crate via `embassy-usb-logger`):

```sh
./scripts/read-serial.sh [seconds]   # default 8
```

### Build variants

```sh
cargo run --release                      # default: Phase C sensor sample loop
cargo run --release --features bringup    # Phase B bring-up diagnostic (VCC/I²C/ADC)
cargo run --release --features wdt-test    # watchdog reset smoke test (resets once)
cargo run --release --features i2c-probe   # continuous 0x38 reads for a logic analyzer
```

### Other scripts

- `scripts/enter-dfu.sh` — manually bounce a running board into DFU over USB.
- `scripts/read-serial.sh` — capture USB-CDC log output.

## Hardware note: AHT20 power-cycle

The AHT20 is power-gated on the P0.13 VCC rail. It **must be power-cycled
cleanly** — drive SDA/SCL low before cutting VCC, let the rail bleed, then
re-init — or it back-powers through its I/O pins and never resets. The
[`aht20.rs`](src/aht20.rs) driver bakes this into every `measure()` call;
verified across consecutive cycles in the default Phase C build.

## Layout

```
src/main.rs        — entry point: Phase C sample loop + auto-DFU handler
                     (the Phase B diagnostic is behind `--features bringup`)
src/aht20.rs       — AHT20 driver (owns the rail + I²C pins; clean power-cycle)
src/battery.rs     — SAADC battery read via the 2N7000-gated divider
src/sensors.rs     — sample_all() aggregator → Sample with Option fields
memory.x           — linker memory regions (gains a SoftDevice region in Phase D)
build.rs           — copies memory.x into OUT_DIR for the linker
.cargo/config.toml — cross-target, UF2 runner, link args
scripts/           — flash-uf2.sh, enter-dfu.sh, read-serial.sh, uf2conv.py
```

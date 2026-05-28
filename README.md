# nrf52840_aht20_rust

Battery-powered BTHome v2 temperature/humidity sensor for Home Assistant.
nRF52840 Pro Micro + AHT20, written in async Rust on Embassy.

See [CLAUDE.md](CLAUDE.md) for project goals and [HARDWARE.md](HARDWARE.md) for
the hardware build spec. The firmware implementation plan lives in
`~/.claude/plans/enchanted-conjuring-dusk.md`.

## Status

**Phase A** — toolchain bring-up. Blinks the onboard LED on P0.15 and logs over
RTT. No sensor, no BLE yet.

## Toolchain

- Rust toolchain: pinned in `rust-toolchain.toml` (auto-installs target on first build).
- Flasher/runner: `probe-rs` ≥ 0.31.

```sh
# One-time, if not already installed:
brew install probe-rs

# Build + flash + run, with RTT logs streamed to the terminal:
cargo run --release
```

`cargo run` is wired to `probe-rs run --chip nRF52840_xxAA` via
`.cargo/config.toml`, so it flashes, resets, and streams `defmt` output in one
step. Connect the board via USB-C with a probe (e.g. another nRF52840-DK, or a
J-Link / CMSIS-DAP). The Pro Micro itself can also be flashed via USB by
holding the reset button to enter bootloader, but that path is not configured
here.

## Layout

```
src/main.rs        — entry point (Phase A: blinky)
memory.x           — linker memory regions (will gain SoftDevice region in Phase D)
build.rs           — copies memory.x into OUT_DIR for the linker
.cargo/config.toml — sets cross-target, runner, and link args
```

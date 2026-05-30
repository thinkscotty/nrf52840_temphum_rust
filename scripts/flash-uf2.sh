#!/usr/bin/env bash
# Cargo runner: ELF -> .uf2 -> copy to mounted UF2 bootloader volume.
#
# Invoked by `cargo run` with the ELF as $1. The Adafruit nRF52 bootloader
# on the Nice! Nano expects:
#   - family ID 0xADA52840 (Nordic NRF52840)
#   - app base address 0x26000 (S140 v6.1.1 slot reserved by bootloader 0.6.0)
#
# Mounted volume defaults to /Volumes/NICENANO; override with UF2_VOLUME.
# If the volume is absent the script still produces the .uf2 and prints
# how to enter the bootloader.

set -euo pipefail

ELF="${1:?usage: flash-uf2.sh <elf>}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UF2_VOLUME="${UF2_VOLUME:-/Volumes/NICENANO}"
FAMILY_ID="0xADA52840"
APP_BASE="0x26000"

BUILD_DIR="$(dirname "$ELF")"
BIN="$BUILD_DIR/firmware.bin"
UF2="$BUILD_DIR/firmware.uf2"

echo "[uf2] objcopy $(basename "$ELF") -> firmware.bin"
rust-objcopy -O binary "$ELF" "$BIN"

echo "[uf2] uf2conv  firmware.bin -> firmware.uf2 (family $FAMILY_ID, base $APP_BASE)"
python3 "$SCRIPT_DIR/uf2conv.py" \
    "$BIN" \
    --convert \
    --family "$FAMILY_ID" \
    --base "$APP_BASE" \
    --output "$UF2" >/dev/null

SIZE=$(wc -c < "$UF2" | tr -d ' ')
echo "[uf2] $UF2 ($SIZE bytes)"

# If the board isn't already in DFU but is running firmware that has the
# DfuHandler, bounce it into the bootloader by sending "dfu" over USB-CDC — no
# reset-button double-tap needed. Falls back to manual instructions below.
if [ ! -d "$UF2_VOLUME" ]; then
    DEV=$(ls /dev/cu.usbmodem* 2>/dev/null | head -1)
    if [ -n "$DEV" ]; then
        echo "[uf2] bouncing $DEV into DFU (sending 'dfu')..."
        printf 'dfu\n' > "$DEV" 2>/dev/null || true
        for _ in $(seq 1 50); do
            [ -d "$UF2_VOLUME" ] && break
            sleep 0.2
        done
        # The freshly-mounted FAT volume isn't write-ready the instant it appears;
        # copying too soon races the mount and the bootloader ignores the UF2.
        [ -d "$UF2_VOLUME" ] && sleep 2
    fi
fi

if [ -d "$UF2_VOLUME" ]; then
    echo "[uf2] copying to $UF2_VOLUME ..."
    # `cp -X` skips macOS extended attributes (FAT can't store them, and
    # the bootloader auto-ejects the volume the instant the UF2 finishes
    # writing — racing the xattr write and producing a misleading error).
    # If the volume vanishes mid-copy that means the flash succeeded, so we
    # tolerate the failure and confirm by checking the volume disappeared.
    cp -X "$UF2" "$UF2_VOLUME/" 2>/dev/null || true
    sync 2>/dev/null || true
    # The bootloader ejects the volume asynchronously once the UF2 is fully
    # written, so poll for the unmount rather than checking instantly.
    for _ in $(seq 1 100); do
        [ -d "$UF2_VOLUME" ] || break
        sleep 0.1
    done
    if [ -d "$UF2_VOLUME" ]; then
        echo "[uf2] WARNING: $UF2_VOLUME still mounted after 10s — flash may not have taken."
        exit 1
    fi
    echo "[uf2] done. Board rebooted into the new firmware."
else
    cat <<EOF
[uf2] $UF2_VOLUME not mounted.
      To flash: double-tap the reset button to enter the bootloader, then re-run
      \`cargo run --release\` (or copy "$UF2" onto the volume manually).
EOF
    exit 1
fi

#!/usr/bin/env bash
# Bounce the running firmware into the UF2 bootloader without touching the board.
#
# The firmware's DfuHandler (src/main.rs) watches the USB-CDC input for "dfu" and,
# on match, sets the Adafruit bootloader's UF2 magic (GPREGRET=0x57) and resets —
# dropping straight into DFU so the NICENANO volume mounts. No reset-button
# double-tap required.
#
# Usage: ./scripts/enter-dfu.sh
#
# Note: only works when the board is running firmware built with the DfuHandler.
# If it's already in DFU (NICENANO mounted) this is a no-op.

if [ -d /Volumes/NICENANO ]; then
    echo "[dfu] already in DFU (NICENANO mounted)."
    exit 0
fi

DEV=$(ls /dev/cu.usbmodem* 2>/dev/null | head -1)
if [ -z "$DEV" ]; then
    echo "[dfu] no /dev/cu.usbmodem* found — board not running, or already in DFU."
    exit 1
fi

echo "[dfu] sending 'dfu' to $DEV ..."
printf 'dfu\n' > "$DEV" 2>/dev/null || true

echo "[dfu] waiting for NICENANO to mount ..."
for _ in $(seq 1 50); do
    [ -d /Volumes/NICENANO ] && { echo "[dfu] in DFU — ready to flash."; exit 0; }
    sleep 0.2
done

echo "[dfu] NICENANO did not mount in 10s."
echo "      (Is this firmware built with the DfuHandler? Otherwise double-tap reset.)"
exit 1

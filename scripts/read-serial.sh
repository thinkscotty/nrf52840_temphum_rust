#!/usr/bin/env bash
# Capture USB-CDC log output from the running firmware.
#
# Waits for a /dev/cu.usbmodem* port to appear (the firmware enumerates one via
# embassy-usb-logger), then prints whatever it sends for DURATION seconds.
#
# Usage: ./scripts/read-serial.sh [duration_seconds]   (default 8)
#
# Note: opening the port with `cat` asserts DTR, which the embassy logger's
# `wait_connection()` requires before it starts streaming. USB-CDC ignores the
# baud rate, so none is set.

DURATION="${1:-8}"

echo "[serial] waiting for /dev/cu.usbmodem* (app must be running, not in DFU)..."
DEV=""
for _ in $(seq 1 30); do
    DEV=$(ls /dev/cu.usbmodem* 2>/dev/null | head -1)
    [ -n "$DEV" ] && break
    sleep 0.5
done

if [ -z "$DEV" ]; then
    echo "[serial] no usbmodem device appeared in 15s."
    echo "         If the board is in DFU (NICENANO mounted), it won't enumerate a serial port."
    exit 1
fi

echo "[serial] reading $DEV for ${DURATION}s ..."
echo "----------------------------------------------------------------"
cat "$DEV" &
CATPID=$!
sleep "$DURATION"
kill "$CATPID" 2>/dev/null
wait "$CATPID" 2>/dev/null
echo "----------------------------------------------------------------"
echo "[serial] done ($DEV)."

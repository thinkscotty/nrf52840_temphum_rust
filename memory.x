/* nRF52840 memory layout — Phase A (no SoftDevice).
 * Will be updated in Phase D to reserve the lower ~156 KiB of flash and
 * the lower ~8 KiB of RAM for the S140 SoftDevice. */
MEMORY
{
  FLASH : ORIGIN = 0x00000000, LENGTH = 1024K
  RAM   : ORIGIN = 0x20000000, LENGTH = 256K
}

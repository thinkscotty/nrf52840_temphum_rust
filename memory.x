/* nRF52840 memory layout — UF2 bootloader (Adafruit/Nice! Nano 0.6.0).
 *
 * Bootloader on this board reports "SoftDevice: S140 version 6.1.1", which
 * reserves 0x00000..0x26000 (152 KiB) for MBR + S140 even when the SoftDevice
 * itself is not yet flashed. The application slot therefore begins at 0x26000.
 *
 * Flash map (per Nice! Nano bootloader 0.6.0 / Adafruit nRF52 bootloader):
 *   0x00000..0x01000  MBR
 *   0x01000..0x26000  S140 v6.1.1 slot (empty until Phase D adds BLE)
 *   0x26000..0xF4000  Application       <-- this region
 *   0xF4000..0xFE000  Bootloader
 *   0xFE000..0xFF000  MBR params
 *   0xFF000..0x100000 Bootloader settings
 *
 * RAM is fully available pre-SoftDevice. When S140 is added in Phase D the
 * lower ~8 KiB of RAM will need to be reserved for the SoftDevice and this
 * file updated accordingly.
 */
MEMORY
{
  FLASH : ORIGIN = 0x00026000, LENGTH = 0xCE000
  RAM   : ORIGIN = 0x20000000, LENGTH = 256K
}

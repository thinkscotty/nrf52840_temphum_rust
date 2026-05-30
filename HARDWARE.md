# Hardware Build Plan — nRF52840 + AHT20 BTHome Sensor

This document is the **build-ready hardware specification** for the smart home temperature/humidity sensor. Build this before writing any firmware.

---

## 1. Design Goals

| Goal | Implication |
|------|-------------|
| **Stability** (1+ year uptime) | Conservative power supply margins, gate-pulldown on every MOSFET, no floating inputs |
| **Strong BLE transmission** | Use onboard PCB antenna; keep antenna area clear of ground pour; nRF radio at +8 dBm |
| **Deep sleep stability** | Power-gate the sensor; eliminate continuous-current paths; allow LiPo to last months despite generous TX power |
| **Battery-powered** | All quiescent paths analyzed; target deep-sleep total system current < 15 µA |

---

## 2. Bill of Materials (BOM)

| Ref | Part | Qty | Notes |
|-----|------|-----|-------|
| U1 | nRF52840 Pro Micro (Teyleten Robot variant — Nice!Nano v2 clone) | 1 | USB-C, onboard LiPo charger, 3.3V LDO, PCB antenna |
| U2 | AHT20 breakout board (I²C) | 1 | Usually has onboard 10 kΩ pull-ups — **verify with multimeter** |
| Q1 | 2N7000 N-channel MOSFET (TO-92) | 1 | Gates battery voltage divider |
| R1 | 100 kΩ, 0.1 % metal-film | 1 | Voltage divider top |
| R2 | 100 kΩ, 0.1 % metal-film | 1 | Voltage divider bottom (matched to R1) |
| R3 | 100 kΩ, 5 % | 1 | Q1 gate pull-down (deterministic OFF at boot) |
| R4 | 1 kΩ, 5 % | 1 | Q1 gate series resistor (limits inrush) |
| R5, R6 | 4.7 kΩ, 5 % | 2 | I²C pull-ups — **only if AHT20 breakout lacks them** |
| C1 | 100 nF ceramic (X7R) | 1 | AHT20 VCC decoupling |
| C2 | 1 nF ceramic (NP0/C0G) | 1 | ADC pin low-pass filter |
| C3 | 1 µF ceramic | 1 | Bulk decoupling near AHT20 |
| BAT1 | 18650 or 21700 LiPo cell | 1 | Single cell, 3.0–4.2 V |
| — | Battery holder, JST-PH 2 mm connector, 30 AWG wire, strip-board perfboard, 3D-printed enclosure | — | — |

---

## 3. Block Diagram

```
                          ┌───────────────────────────────┐
   USB-C ───────────────►│ nRF52840 Pro Micro (U1)        │
                          │  • Onboard LiPo charger        │
   BAT1 ──┬─────► B+ pin ►│  • 3.3 V LDO                   │
          │               │  • PCB antenna                 │
          │               │  • VCC = gated 3.3 V rail      │
          │               └──┬────────┬───────┬────────────┘
          │                  │ VCC    │ SDA   │ SCL
          │                  ▼        ▼       ▼
          │              ┌─────────────────────┐
          │              │ AHT20 (U2)          │
          │              └─────────────────────┘
          │                 decoupling at the AHT20 pins (across VCC↔GND):
          │                 VCC ──┬──[ C1 = 100nF ]──┬── GND
          │                       └──[ C3 = 1µF ]────┘
          │
          ├─[ R1 = 100k ]──┬──────────────► ADC (P0.31 / AIN7)
          │                │
          │                ├──[ C2 = 1nF ]──── GND   (shunt filter — NOT in series)
          │                │
          │             [ R2 = 100k ]
          │                │
          │                ▼
          │             Drain (Q1, 2N7000)
          │              Source ─── GND
          │              Gate ──[ R4 = 1k ]── GPIO_EN (P0.22)
          │                      │
          │                     [ R3 = 100k pull-down to GND ]
          │
         GND
```

---

## 4. Pin Assignments

> ⚠️ **Verification step before soldering:** The Teyleten Robot board's silkscreen-to-GPIO mapping is *mostly* identical to Nice!Nano v2 but is community-reverse-engineered. Cross-check against the diagram at <https://github.com/longrackslabs/teyleten-nrf52840-pinout> and confirm with a continuity test from each silkscreen pad to the corresponding nRF pin if possible.

| Function | nRF GPIO | Typical Silkscreen | Rationale |
|----------|----------|-------|-----------|
| **I²C SDA → AHT20** | `P0.17` | `D2` | Standard Nice!Nano I²C pair; regular-drive pin; physically adjacent to SCL |
| **I²C SCL → AHT20** | `P0.20` | `D3` | Pair with SDA above |
| **Battery ADC input** | `P0.31` (AIN7) | `18` | Hardware analog input; far from I²C bus to minimize noise; AIN7 SAADC channel |
| **Battery sense enable (Q1 gate)** | `P0.22` | `D4` | Regular-drive GPIO; defaults to high-Z at boot (R3 holds Q1 off) |
| **AHT20 power** | gated `VCC` rail | `VCC` | Driven by onboard P0.13 internal control — power-gates the sensor between samples |
| **VCC rail enable (internal)** | `P0.13` | n/a | **Polarity check needed.** Nice!Nano docs say HIGH = OFF; Teyleten README says HIGH = ON. Test with a known-good LED on VCC before relying on it. |
| **Onboard user LED** | `P0.15` (Zephyr standard) | n/a | **Disable in firmware** for production to save current |
| Unused: USB | `P0.24`, `P0.25` | n/a | Keep configured for USB during flashing |

**Pins to avoid:**
- `P0.14`, `P0.16` — flagged as "reserved / causes system issues" on the Teyleten clone
- `P0.04` — used by Nice!Nano's onboard battery divider (and a known-broken pin on some clones)
- `P0.09`, `P0.10` — default NFC pins; require firmware to release them
- `P0.13` — controls the VCC rail; never repurpose

---

## 5. Subsystem Schematics

### 5.1 Power Distribution

```
   USB-C ┐                           ┌──► VBUS pin (5 V)
         │  Onboard TP4056-class     │
         ├─►  LiPo charger ◄─────────┤
         │                           │
   BAT1 ─┴──────────────► B+ pin ────┴──► onboard 3.3 V LDO ──► 3V3 pin (always on)
                                                              │
                                                              └─► P0.13-gated ──► VCC pin
```

- The Pro Micro board handles charging and regulation. You only wire **BAT1 +** to **B+** and **BAT1 −** to **GND**.
- **AHT20 VCC connects to `VCC` (the gated rail), not `3V3`.** This lets firmware kill sensor power between samples.
- The voltage divider's top **must connect to `B+`** (raw battery) — not to `VCC`/`3V3`, because we want to measure the actual cell voltage, not the LDO output.

### 5.2 AHT20 Wiring

```
AHT20    →   Pro Micro
─────────────────────────
VCC      →   VCC  (gated rail)
GND      →   GND
SDA      →   P0.17 (D2)
SCL      →   P0.20 (D3)
```

- Place **C1 (100 nF) + C3 (1 µF)** across AHT20 VCC ↔ GND, as close to the AHT20 pins as physically possible.
- **I²C pull-ups:** Most AHT20 breakouts already include 10 kΩ pull-ups on SDA/SCL.
  - Test the breakout with a multimeter from SDA → VCC and SCL → VCC. Expect a finite resistance (typically 5–10 kΩ).
  - **If pull-ups are absent**, add R5/R6 = 4.7 kΩ from SDA/SCL to the **VCC (gated) rail** (not 3V3 — pull-ups must die with the sensor or they leak current into the chip during sleep).
- AHT20 boot time after power-on: ~40 ms typical. Firmware must wait at least 100 ms before the first measurement command.
- **Desolder the onboard power LED** (and/or its series resistor) on the AHT20/21 breakout. The LED is already dark during sleep because P0.13 cuts the gated VCC rail, but during the ~100 ms measurement window each minute it would otherwise draw ~2–5 mA (≈3–8 µA averaged over the cycle) and produce a visible blink. Removing it saves that draw and keeps the sensor stealthy in living spaces.

### 5.3 Battery Voltage Divider (gated)

```
       B+ (battery raw, 3.0–4.2 V)
        │
       [R1 = 100 kΩ 0.1 %]
        │
        ├─────────────────────► P0.31 (AIN7)
        │              │
        │            [C2 = 1 nF]
        │              │
        │             GND
        │
       [R2 = 100 kΩ 0.1 %]
        │
        ├─► Drain (Q1, 2N7000)
            Source ─── GND
            Gate  ◄─[R4 = 1 kΩ]─ P0.22 (D4)
                  │
                 [R3 = 100 kΩ]
                  │
                 GND
```

**Q1 (2N7000) physical pinout — TO-92 package:**

Hold the part with the **flat face toward you**, leads pointing **down**:

```
       ┌─────────┐
       │  2N7000 │   ← flat (labelled) face
       │         │
       └─┬──┬──┬─┘
         S  G  D
       (left, middle, right)
```

| TO-92 lead | MOSFET terminal | Connects to |
|------------|-----------------|-------------|
| Left  | **Source** | GND |
| Middle | **Gate**   | R4 (1 kΩ) → P0.22, *and* R3 (100 kΩ) → GND |
| Right | **Drain**  | Bottom of R2 (the "out" side of the second resistor) |

> ⚠️ Verify the pinout against the datasheet for the **specific 2N7000 you bought** — the original Fairchild/onsemi part uses S-G-D left-to-right, but some no-name clones occasionally swap pins. Diode-test with a multimeter: probing Drain (red) → Source (black) should show ~0.6–0.8 V drop (the body diode) in one direction only; Gate-to-anything reads open.

**How it works:**
- When firmware drives `P0.22` **HIGH** → Q1 conducts (Drain–Source channel opens) → divider has a path to GND → `P0.31` reads ≈ `Vbat / 2`.
- When firmware drives `P0.22` **LOW** (or floating at boot) → R3 pulls Gate to GND → Q1 off → divider is open-circuit at the bottom.
- Firmware reads battery, then immediately drives `P0.22` LOW again. Total measurement time: a few ms.

**Conceptual note:** In an N-channel MOSFET, conventional current flows **Drain → Source** when on. The Source is always the *lower-potential* terminal (GND in a low-side switch). The Gate is just a control input — almost no current flows into it; the voltage between Gate and Source (Vgs) is what turns the channel on (above the threshold, ~2 V for a 2N7000) or off (below it).

**Quiescent current analysis (Q1 OFF, deep sleep):**
- The divider's bottom is floating; no DC path to ground through R1/R2.
- The ADC node sits at ≈ V_bat, which exceeds VDD (3.3 V). The chip's ESD clamp diode keeps the pin at ≈ VDD + 0.3 V.
- Resulting leakage through R1 into the supply rail: `(4.2 − 3.6) / 100 kΩ ≈ 6 µA` worst case.
- A 21700 cell at 4000 mAh shows roughly **3 %/month self-discharge** (≈ 165 µA equivalent). So the divider leakage is ~4 % of the cell's own self-discharge — **acceptable**, and far less than the AHT20's quiescent draw if it weren't power-gated.
- **If you want to eliminate even this:** swap R1/R2 for 1 MΩ matched pair and increase ADC acquisition time to 40 µs in firmware. Leakage drops to ~0.6 µA.

**Calibration:**
- Measure R1 and R2 with a 4½-digit DMM, record the actual ratio, hardcode into firmware. With 0.1 % parts, the worst-case error is ~0.2 %, which at 4.2 V is ~8 mV — well within usable resolution.
- Apply a 2-point calibration: read divider with a known supply at 3.0 V and 4.2 V, store `(adc_min, adc_max)` in firmware.

---

## 6. Power Budget Estimate

| Mode | Duration / cycle | Current | Notes |
|------|------------------|---------|-------|
| nRF52840 System OFF, RAM retained | ~59 s of every 60 s | ~2 µA | Goal — verified achievable on this chip |
| Voltage-divider leakage | continuous | ~6 µA | See §5.3; can be reduced |
| AHT20 power-gated OFF | continuous | ~0 µA | VCC rail disabled via P0.13 |
| **Idle subtotal** | | **~8 µA** | |
| AHT20 wake + measure (with power-on delay) | ~100 ms per cycle | ~1.5 mA | Dominated by AHT20 startup |
| Battery ADC sample burst | ~5 ms | ~25 µA (divider) + chip ADC | Negligible per cycle |
| BLE advertise (3 packets, +8 dBm, ~10 ms) | ~10 ms per cycle | ~10 mA peak | Strongest TX power for range |
| **Per-cycle active average** | | ≈ 75 µA over 60 s | (1.5 mA × 0.1 s + 10 mA × 0.01 s) ÷ 60 s |
| **Total average** | | **~83 µA** | |

**Battery life estimate:**
- 21700 cell at 4000 mAh → 4000 / 0.083 ≈ **48 000 hours ≈ 5.5 years** theoretical.
- Self-discharge and capacity fade will cap real-world life at ~2–3 years before voltage drops below useful range. Comfortably beats the 1-year goal.

---

## 7. Strip-Board / Perfboard Layout Suggestions

- Keep the **antenna end** of the Pro Micro (the side opposite the USB-C connector) overhanging the edge of the perfboard, or cut/remove copper underneath it. Ground plane near the chip antenna detunes it and ruins range.
- Group the **AHT20 close to a board edge** with a vent slot in the enclosure for accurate humidity readings — heat from the nRF52840 will bias readings high if AHT20 is too close.
- Place the **voltage divider compactly** near the battery connector, then run a single trace to the ADC pin. Keep this trace short to minimize noise pickup.
- The **MOSFET gate trace** can be long without issue, but include R4 (1 kΩ) at the GPIO end.
- Add a **TP (test point) pad on the ADC node** — invaluable for calibration with a multimeter.
- Add a **TP pad on the gated VCC rail** — lets you verify P0.13 polarity before trusting the firmware.

---

## 8. Pre-Build Verification Checklist

Before powering up:
- [ ] Continuity test: BAT1+ → B+ pin, BAT1− → GND pin (no shorts, < 1 Ω)
- [ ] Continuity test: no short between 3V3 and GND (should read >10 kΩ in either direction)
- [ ] Continuity test: ADC node to either GND or B+ should NOT be < 100 kΩ (verifies R1/R2 are populated)
- [ ] Q1 orientation: Source → GND, Gate → P0.22 via R4, Drain → bottom of R2 (TO-92, flat face toward you, leads down: **S–G–D left to right**; see §5.3 for diagram). **Verify with datasheet for your specific 2N7000 vendor.**
- [ ] AHT20 breakout pull-ups present? If not, add R5/R6.
- [ ] AHT20 onboard power LED desoldered (or its series resistor removed) — see §5.2.

First power-on (no firmware yet):
- [ ] Connect USB-C with battery disconnected. Verify 3V3 pin reads 3.3 V ± 5 %.
- [ ] Disconnect USB, connect battery. Verify 3V3 still reads ≈ 3.3 V.
- [ ] With no firmware controlling P0.22, ADC node should sit at ≈ V_bat (divider's bottom is open). This confirms R3 is correctly holding Q1 off at rest.
- [ ] VCC rail polarity test: load the default firmware on the board and check whether VCC is HIGH or LOW at boot. Document the polarity in `CLAUDE.md` before writing real firmware.

---

## 9. Open Questions

**Resolved on the bench during Phase B** (see CLAUDE.md → Bench-Confirmed Facts):

1. ✅ **P0.13 polarity for VCC rail: HIGH = ON** (LOW = off). The rail self-bleeds to ~0 when P0.13 goes low — no discharge part needed.
2. ✅ **AHT20 pull-ups present** (~10 kΩ on SDA/SCL).
3. ✅ **Pin mapping confirmed for the pins we use** — P0.13 (VCC), P0.17 (SDA), P0.20 (SCL), P0.22 (Q1-enable), P0.31 (AIN7) all verified by working firmware. ⚠️ **SCL (P0.20) and the Q1-enable (P0.22) are physically adjacent** — a solder bridge shorted them and silently killed I²C; keep that route clear.

**New finding (Phase B):** the AHT20 **wedges if VCC is gated naively** — it back-powers through SDA/SCL and never POR-resets. It must be **power-cycled cleanly**: drive SDA+SCL low → cut VCC → bleed → re-power → re-init. Baked into the Phase C driver.

**Still open (firmware decision, not a hardware unknown):**

4. **Battery voltage cutoff threshold.** Decide the minimum cell voltage at which the firmware stops transmitting (typical: 3.0 V to protect the cell from over-discharge).

---

## 10. References

- Teyleten Robot pin mappings: <https://github.com/longrackslabs/teyleten-nrf52840-pinout>
- Nice!Nano v2 pinout: <https://nicekeyboards.com/docs/nice-nano/pinout-schematic/>
- nRF52840 SAADC channel mapping: AIN0=P0.02, AIN1=P0.03, AIN2=P0.04, AIN3=P0.05, AIN4=P0.28, AIN5=P0.29, AIN6=P0.30, AIN7=P0.31
- BTHome v2 advertising format: <https://bthome.io/format/> (service UUID `0xFCD2`, temperature ID `0x02`, humidity `0x03`, voltage `0x0C`)
- nRF52840 reverse-engineering / sleep current quirks: <https://github.com/sasodoma/nrf52840-promicro>

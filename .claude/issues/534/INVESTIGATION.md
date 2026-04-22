# #534 Investigation — M33-02 Cloud FourCCs

## Domain
**esm** — `crates/plugin/src/esm/records/weather.rs`.

## Root cause

Parser matches `00TX/10TX/20TX/30TX`. Actual FNV/FO3/Oblivion WTHR cloud
texture sub-records have FourCCs `DNAM/CNAM/ANAM/BNAM`. Histograms from
audit's deleted scratch harness:

| FourCC | FNV (63 WTHRs) | FO3 (27) | Oblivion (37) |
|---|---|---|---|
| `DNAM` | 63/63 | 27/27 | 35/37 |
| `CNAM` | 61/63 | 27/27 | 36/37 |
| `ANAM` | 62/63 | 27/27 | 0/37 |
| `BNAM` | 61/63 | 27/27 | 0/37 |
| `00TX/10TX/20TX/30TX` | **0** | **0** | **0** |

Oblivion ships 2 cloud layers (CNAM + DNAM only), FNV/FO3 ship 4.

## Layer ordering

Ordered by the order Bethesda emits them in real WTHRs (schema order):
`DNAM → CNAM → ANAM → BNAM` = layers `0 → 1 → 2 → 3`.

Empirical confirmation: WTHR #0 (a `NVWastelandClear*` variant) emits
sub-records in exactly that order. Per-record evidence that DNAM is
layer 0:
- `NVWastelandClear` DNAM = "sky\alpha.dds" (placeholder)
- Similar record #1 DNAM = "sky\NVWastelandS…" (real primary cloud)
- Similar record #2 DNAM = "sky\wastelandclo…" (real primary cloud)

DNAM holds the primary cloud texture most often → consistent with being
layer 0 (the one scene.rs:216 picks up as `cloud_textures[0]`).

## Files touched

1. `crates/plugin/src/esm/records/weather.rs` — parser arms flip, unit test updated
2. Also affects #535 (DNAM can't be both a path and a speed array — must fix together)

---

# #535 Investigation — M33-03 DNAM semantics

## Domain
**esm** + **binary** (downstream consumer).

## Root cause

DNAM arm reads `sub.data[0..4]` as `[u8; 4]` cloud speeds. But DNAM is a
cloud-texture-path zstring — see #534. Byte evidence:
`DNAM size=14 peek="sky\alpha.dds\0"` → parser reads `[0x73, 0x6b, 0x79,
0x5c] = [115, 107, 121, 92]` = ASCII `"sky\"`, then divides by 128 in
`weather_system`, yielding a scroll rate of ≈0.018 UV/sec.

## Consumer

`byroredux/src/systems.rs:1277`:
```rust
let cloud_speed_01 = wd.cloud_speeds[0] as f32 / 128.0;
let cloud_scroll_rate = 0.02 * cloud_speed_01;
```

## Fix

Remove the `cloud_speeds: [u8; 4]` field from `WeatherRecord` and
`WeatherDataRes`. Delete the DNAM-as-speeds arm. Hardcode
`cloud_scroll_rate` in `weather_system` to match the pre-fix
observed-good value (`0.018 UV/sec` — empirically set, per audit note
the coincidence that `"s"=0x73=115` normalised to `0.898` multiplied
the authored `0.02` to ≈0.018). A follow-up issue will need to source
the real scroll rate from UESP-authoritative sub-records (ONAM? PNAM?
INAM?) — flag that in a comment but don't open the issue yet (M33-09 /
#541 already covers unused WTHR fields).

## Files touched

1. `crates/plugin/src/esm/records/weather.rs` — field removal, arm removal
2. `byroredux/src/components.rs` — WeatherDataRes field removal
3. `byroredux/src/scene.rs` — stop passing `cloud_speeds`
4. `byroredux/src/systems.rs` — hardcode scroll rate, comment the deferral

## Combined scope: 4 files. Under threshold.

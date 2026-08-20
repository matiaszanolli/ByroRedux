# #3144 — ESM-2026-08-20-D5-05: `wind_direction` is read in degrees and stored into a radians field on the Oblivion and FO3/FNV arms — every river flows on one wrong bearing

**Finding**: ESM-2026-08-20-D5-05
**Labels**: bug, import-pipeline, medium, legacy-compat
**Filed**: 2026-08-20 · `/audit-publish` · HEAD `bb0b92f2`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/3144

---

- **Severity**: MEDIUM
- **Dimension**: ESM Dim 5 — CELL / WRLD walkers (WATR record schema)
- **Record / Sub-record**: `WATR` / `DNAM` (FO3/FNV), `WATR` / `DATA` (Oblivion)
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:707` (`decode_dnam_pre_fo4`), `:472` (`decode_data_oblivion`'s field loop, `(&mut p.wind_direction, 4usize)`), `:373` (`decode_data`'s short compatibility prefix); field contract at `:168-169`; consumer `byroredux/src/env_translate.rs:951-963`
- **Status**: NEW

## Description

`WaterParams::wind_direction` is documented at `water.rs:168` as *"Wind direction in radians"*, and `env_translate.rs:951-953` restates the contract in situ — *"Bethesda's `wind_direction` is in radians from north (UESP)"* — before doing `theta.sin_cos()` to synthesise the flow heading for every non-`Calm` water body.

Two decoders assign the raw `f32` straight off the wire with no `.to_radians()`:

- `decode_dnam_pre_fo4:707` — `p.wind_direction = v;`
- `decode_data_oblivion:472` — `(&mut p.wind_direction, 4usize)` in the bare copy loop

Every *other* direction producer in the same file converts. `decode_data_fo3nv:630-640`, `apply_skyrim_dnam_tail:808-818`, `decode_dnam_fo4:1002-1012` and `decode_dnam_starfield:1223-1264` all read the noise-layer directions with `.to_radians()` and then assign `p.wind_direction = p.noise_wind_directions[0]`.

## Evidence

Byte census of the raw field across vanilla masters (independent Python GRUP walks, not the parser):

```
Oblivion.esm   DATA[4]  → 35.0 / 100.0 / 90.0          (n=17 full-length records)
Fallout3.esm   DNAM[4]  → 90.0 on 53/53
FalloutNV.esm  DNAM[4]  → 90.0 on 69/69
```

Values in `[0, 360)` with a `90.0` mode are degrees. `90.0 rad mod 2π ≈ 2.04 rad ≈ 117°`.

**Which games are actually affected — verified arm by arm at HEAD:**

| Game | Path | Overwritten later? | Affected |
|---|---|---|---|
| Oblivion | `decode_data_oblivion` | no — `noise_wind_directions[0]` is derived by `atan2` from the scroll pair at 28/32 (`:494`), and `wind_direction` is never reassigned | **YES** |
| FO3 / FNV `DNAM` (196 B, the majority carrier) | `decode_dnam_pre_fo4` only | no tail pass runs for `Fallout3NV` | **YES** |
| FO3 / FNV `DATA` (186 B) | `decode_data_fo3nv` | yes — `:640` reassigns from the `.to_radians()`-converted noise layer 1 | no |
| **Skyrim** | `decode_dnam_pre_fo4` **+** `apply_skyrim_dnam_tail` | **yes** — `:818` reassigns from the converted noise layer 1 | **NO — do not claim Skyrim** |
| FO4 / FO76 / Starfield | own arms | yes | no |

The Skyrim exemption is load-bearing: an earlier draft of this finding claimed Skyrim was affected. It is not — `apply_skyrim_dnam_tail` overwrites the raw prefix value in radians. Any fix must not "also fix" Skyrim.

`decode_data:373` has the same raw assignment, but that path is dead on all vanilla data (see the companion dead-tail issue).

## Impact

Every classified flowing water body on Oblivion, Fallout 3 and Fallout New Vegas gets the *same wrong* current heading. `env_translate.rs:962` turns the 90.0 into `≈2.04 rad`, so every `River` / `Rapids` plane in every worldspace flows on one fixed bearing regardless of what the record authored. FNV is the project's declared reference title.

This is the surviving half of the `watr_data_layout_shift` project-memory note — the Oblivion fog/colour half of that note is now genuinely fixed and verified (fog at DATA 36/40, byte colours at 44/48/52, confirmed on 17/17 records).

## Related

- #3107 — the FO3/FNV `DNAM` prefix stops at byte 52, which is *why* no tail pass ever runs to correct `wind_direction` on that path. Adopting #3107's shared-tail fix subsumes this one for FO3/FNV but **not** for Oblivion.
- #3104, #3105, #3108, #3110 — sibling WATR offset defects from the same arbitration.
- **Not** #2872 (CLOSED), which was the shader-scroll-vs-BU/s *unit* seam on `WaterFlow::speed`, not the parse-side angle.

## Suggested Fix

Prefer the noise-layer-1 direction (already `.to_radians()`-converted) on both arms, exactly as every modern arm does. If the legacy prefix field is kept as a fallback for records with no noise layers, convert it with `.to_radians()` at the assignment. Leave the Skyrim arm alone.

Pin with a real-data assertion (not a synthetic round-trip): on a named `FalloutNV.esm` `River` record, `wind_direction` must be in `[0, 2π)` and must not equal `90.0`.

---
*Filed from `docs/audits/AUDIT_ESM_2026-08-20.md` (Dim 5). Verified against HEAD `bb0b92f2` before filing.*

## Completeness Checks
- [ ] **SIBLING**: every `wind_direction` / `noise_wind_directions` assignment in `crates/plugin/src/esm/records/misc/water.rs` checked for the same missing conversion (there are 6 producer sites)
- [ ] **CANONICAL-BOUNDARY**: the unit conversion stays in the WATR decoder — never pushed into `resolve_water_material`, `render/water.rs` or `water.frag`. See `/audit-nifal`.
- [ ] **NO-REGRESSION-SKYRIM**: the Skyrim arm already resolves this correctly via `apply_skyrim_dnam_tail:818`; do not add a second conversion there
- [ ] **NO-ANALOGY**: the WATR-side `FNAM` bit `0x10` decode is empirically correct (`DefaultWaterFlow` 0x08 vs `DefaultWaterFlowBlend` 0x18) and must not be "fixed" by analogy with the undefined NIF-side `blend_normals` bit 16 — different bits, different formats, fixing one by analogy with the other breaks working code
- [ ] **TESTS**: a regression test pins this against *shipped bytes*, not against the decoder's own output

# WATR-ARB-05: `decode_data_fo3nv` sources `normal_magnitude` from `DATA[96]`, which is uninitialised on some records and an 8× amplifier on others

Filed: 2026-08-20 · Source: `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` · GitHub: #3108

- **Severity**: LOW
- **Location**: `water.rs:604-608`
- **Status**: NEW
- **Evidence**: across all long-`DATA` records, offset 96 reads `0xCDCDCDCD` (`-4.316e8`, MSVC uninitialised fill) on 3/11 FO3 and 2/8 FNV records, and otherwise `0.36 / 0.4 / 0.7 / 1.5 / 1.8 / 7.25 / 9.1`. The Skyrim decoder calls the same offset `noise_falloff`; the two cannot both be right, and Skyrim's own `[96]` distribution (0 / 4 / 100 / 445 / 1009 / 3770 / 4007 / 5000 / 8192) matches the FO76/Starfield noise-falloff family (4096 / 8192 / 100), while FO3/FNV's does not.
- **Impact**: negative reads fall back to the neutral `1.0`, but `9.1` clamps to `8.0` and multiplies all three noise amplitudes 8×. Bounded to the ≤19 long-`DATA` records.
- **Fix**: drop the assignment; leave `normal_magnitude` neutral until offset 96 is byte-decoded for the Fallout layout specifically.
---
*Filed from `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` — a byte-level arbitration run to resolve a direct contradiction between `/audit-fo4` and `/audit-legacy-compat` during the 2026-08-20 comprehensive suite. Authority is shipped bytes from all seven vanilla masters plus the GECK/CK default simulator tuple; `find / -iname "*Records.pas"` returns zero hits, so no xEdit definition exists on this machine.*

## Completeness Checks
- [ ] **SIBLING**: the same offset pattern checked in every other per-game WATR decoder in `crates/plugin/src/esm/records/misc/water.rs` (they do not share a helper)
- [ ] **CANONICAL-BOUNDARY**: per-game layout logic stays in the WATR decoder — never pushed into `resolve_water_material`, `render/water.rs` or `water.frag`. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins this fix against *shipped bytes*, not against the decoder's own output — three existing fixtures encode the current inversion and cannot catch it (see WATR-ARB-06)
- [ ] **NO-ANALOGY**: the WATR-side `FNAM` bit `0x10` decode is empirically correct (`DefaultWaterFlow` 0x08 vs `DefaultWaterFlowBlend` 0x18) and must not be "fixed" by analogy with the undefined NIF-side `blend_normals` bit 16

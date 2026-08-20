# WATR-ARB-07: the 86-byte Oblivion `DATA` variant reads a falloff as `wave_amplitude`

Filed: 2026-08-20 · Source: `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` · GitHub: #3110

- **Severity**: LOW
- **Location**: `water.rs:512-521`
- **Status**: NEW
- **Evidence**: 2/23 `Oblivion.esm` records ship an 86-byte `DATA` (`SwampWater`, `MS31Water`). Their bytes at 60..80 are `0.1 0.6 0.985 | 0.4 0.6 0.985` — two three-float simulators (force/velocity/falloff, no dampener or starting size) followed by the `u16` damage at 84, which is exactly 86 bytes. `decode_data_oblivion` reads `wave_amplitude ← [80]` = `0.985` (a falloff) and `wave_frequency ← [84]` = out of range.
- **Impact**: two records; both get a nonsensical `wave_amplitude`.
- **Fix**: gate the simulator reads on `data.len() >= 102`, or add the short-form arm.

---
---
*Filed from `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` — a byte-level arbitration run to resolve a direct contradiction between `/audit-fo4` and `/audit-legacy-compat` during the 2026-08-20 comprehensive suite. Authority is shipped bytes from all seven vanilla masters plus the GECK/CK default simulator tuple; `find / -iname "*Records.pas"` returns zero hits, so no xEdit definition exists on this machine.*

## Completeness Checks
- [ ] **SIBLING**: the same offset pattern checked in every other per-game WATR decoder in `crates/plugin/src/esm/records/misc/water.rs` (they do not share a helper)
- [ ] **CANONICAL-BOUNDARY**: per-game layout logic stays in the WATR decoder — never pushed into `resolve_water_material`, `render/water.rs` or `water.frag`. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins this fix against *shipped bytes*, not against the decoder's own output — three existing fixtures encode the current inversion and cannot catch it (see WATR-ARB-06)
- [ ] **NO-ANALOGY**: the WATR-side `FNAM` bit `0x10` decode is empirically correct (`DefaultWaterFlow` 0x08 vs `DefaultWaterFlowBlend` 0x18) and must not be "fixed" by analogy with the undefined NIF-side `blend_normals` bit 16

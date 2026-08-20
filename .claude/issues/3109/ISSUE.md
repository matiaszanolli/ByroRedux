# WATR-ARB-06: three fixtures pin the swapped labelling as expected behaviour

Filed: 2026-08-20 · Source: `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` · GitHub: #3109

- **Severity**: LOW
- **Location**: `water.rs:1574,1579,1589-1590` (Oblivion); `:1622-1625,1645-1646` (FO3/FNV); `:1736,1738,1780,1788` (Skyrim)
- **Status**: NEW
- **Evidence**: quoted in full under sub-question 5, including the direct contradiction with `:1855-1859` / `:1904` (FO4) over the identical five offsets.
- **Fix**: correct the three fixtures alongside WATR-ARB-01/02, and add the real-data guard Claim A proposed — assert in `crates/plugin/tests/parse_real_esm.rs` that no scalar folded into `noise_amplitude_scales` is invariant across a game's whole WATR population. Invariance across 34 authored records is the signal that caught this.
---
*Filed from `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` — a byte-level arbitration run to resolve a direct contradiction between `/audit-fo4` and `/audit-legacy-compat` during the 2026-08-20 comprehensive suite. Authority is shipped bytes from all seven vanilla masters plus the GECK/CK default simulator tuple; `find / -iname "*Records.pas"` returns zero hits, so no xEdit definition exists on this machine.*

## Completeness Checks
- [ ] **SIBLING**: the same offset pattern checked in every other per-game WATR decoder in `crates/plugin/src/esm/records/misc/water.rs` (they do not share a helper)
- [ ] **CANONICAL-BOUNDARY**: per-game layout logic stays in the WATR decoder — never pushed into `resolve_water_material`, `render/water.rs` or `water.frag`. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins this fix against *shipped bytes*, not against the decoder's own output — three existing fixtures encode the current inversion and cannot catch it (see WATR-ARB-06)
- [ ] **NO-ANALOGY**: the WATR-side `FNAM` bit `0x10` decode is empirically correct (`DefaultWaterFlow` 0x08 vs `DefaultWaterFlowBlend` 0x18) and must not be "fixed" by analogy with the undefined NIF-side `blend_normals` bit 16

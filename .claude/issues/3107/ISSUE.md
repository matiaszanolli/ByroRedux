# WATR-ARB-04: the majority FO3/FNV path (`DNAM`, 196 B) stops decoding at byte 52

Filed: 2026-08-20 · Source: `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` · GitHub: #3107

- **Severity**: MEDIUM
- **Location**: `water.rs:1359` (the `_ =>` dispatch arm) → `decode_dnam_pre_fo4` (`:694-753`)
- **Status**: NEW. Missed by both claims — Claim A's and Claim B's FO3/FNV analyses both target `decode_data_fo3nv`, which vanilla FO3/FNV reaches on only a minority of records.
- **Evidence**: sub-record census. FO3: 11 records carry a 186-byte `DATA` (→ `decode_data_fo3nv`), **42 carry a 196/184-byte `DNAM`** (→ `decode_dnam_pre_fo4`). FNV: 8 vs **70**. The two forms never co-occur on the same record (0/53 and 0/78). `decode_dnam_pre_fo4` reads bytes 0..52 and returns.
- **Impact**: on 79% of FO3 and 90% of FNV vanilla water types the rain simulator, displacement simulator, three noise layers, fog amounts, underwater fog pair, noise UV scales, amplitudes and the specular tail are all left at canonical defaults. The `DNAM` head is otherwise correct — verified against shipped bytes: `[0]`=wind speed (3.0), `[4]`=direction (90), `[8]/[12]`=wave amp/freq (0.2 / 0.25), `[16]`=sun power (826), `[20]`=reflectivity, `[24]`=fresnel, `[28]`=unnamed 0, `[32]/[36]`=fog near/far (−80 / 850), `[40]/[44]/[48]`=packed RGBA.
- **Fix**: route `GameKind::Fallout3NV` `DNAM` through the same tail decode as the 186-byte `DATA` (the two are offset-identical from byte 56 onward — verified: both carry `0.1 0.6 0.985 2 0.01 | 0.4 0.6 0.985 10 0.05` at 56..92).
---
*Filed from `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` — a byte-level arbitration run to resolve a direct contradiction between `/audit-fo4` and `/audit-legacy-compat` during the 2026-08-20 comprehensive suite. Authority is shipped bytes from all seven vanilla masters plus the GECK/CK default simulator tuple; `find / -iname "*Records.pas"` returns zero hits, so no xEdit definition exists on this machine.*

## Completeness Checks
- [ ] **SIBLING**: the same offset pattern checked in every other per-game WATR decoder in `crates/plugin/src/esm/records/misc/water.rs` (they do not share a helper)
- [ ] **CANONICAL-BOUNDARY**: per-game layout logic stays in the WATR decoder — never pushed into `resolve_water_material`, `render/water.rs` or `water.frag`. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins this fix against *shipped bytes*, not against the decoder's own output — three existing fixtures encode the current inversion and cannot catch it (see WATR-ARB-06)
- [ ] **NO-ANALOGY**: the WATR-side `FNAM` bit `0x10` decode is empirically correct (`DefaultWaterFlow` 0x08 vs `DefaultWaterFlowBlend` 0x18) and must not be "fixed" by analogy with the undefined NIF-side `blend_normals` bit 16

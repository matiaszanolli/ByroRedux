# WATR-ARB-03: `decode_dnam_fo76` decodes a layout FO76 does not use; `displacement` receives wind-direction degrees

Filed: 2026-08-20 · Source: `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` · GitHub: #3106

- **Severity**: MEDIUM
- **Location**: `water.rs:1059-1155`
- **Status**: NEW. **Contradicts both sibling claims**, each of which certified this function as correct.
- **Evidence**: FO76's 148-byte `DNAM` is structurally identical to Starfield's 152-byte `DNAM` over bytes 0..144 (table above). Measured over all 47 records: `[80] == 0.05` ×47 (the displacement starting size the decoder never reads), `[48]` has 25 distinct values (the real normal magnitude, never read), `[52] == 1.0` ×47 (what the decoder *does* read as normal magnitude), and `[84]/[88]/[92]` are degrees (15/17/20 distinct, e.g. 239.328 / 331.848 / 62.352) — which the decoder assigns to `displacement[1]/[2]/[0]` and `env_translate` then clamps into `[0, 10000]`. It also reads `reflectivity ← [64]` (displacement force 0.1), `fresnel ← [68]` (displacement velocity 0.85), `wave_amplitude ← [76]` (displacement dampener 0.97), `wave_frequency ← [80]` (starting size 0.05), and `read_rgb_at(4)/(8)` over what are float triples, not packed RGBA (FO4's *are* packed RGBA — `3a 35 21 00` — which is what makes the two layouts distinguishable).
- **Impact**: every FO76 water body's colour, fog, reflectivity, Fresnel, wave motion and ripple profile is decoded from the wrong fields. MEDIUM rather than HIGH only because ROADMAP lists FO76 as archive/NIF-parse coverage with no shipped playable cell.
- **Fix**: rebase `decode_dnam_fo76` on `decode_dnam_starfield`'s offset map, minus the trailing roughness at 148.
---
*Filed from `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` — a byte-level arbitration run to resolve a direct contradiction between `/audit-fo4` and `/audit-legacy-compat` during the 2026-08-20 comprehensive suite. Authority is shipped bytes from all seven vanilla masters plus the GECK/CK default simulator tuple; `find / -iname "*Records.pas"` returns zero hits, so no xEdit definition exists on this machine.*

## Completeness Checks
- [ ] **SIBLING**: the same offset pattern checked in every other per-game WATR decoder in `crates/plugin/src/esm/records/misc/water.rs` (they do not share a helper)
- [ ] **CANONICAL-BOUNDARY**: per-game layout logic stays in the WATR decoder — never pushed into `resolve_water_material`, `render/water.rs` or `water.frag`. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins this fix against *shipped bytes*, not against the decoder's own output — three existing fixtures encode the current inversion and cannot catch it (see WATR-ARB-06)
- [ ] **NO-ANALOGY**: the WATR-side `FNAM` bit `0x10` decode is empirically correct (`DefaultWaterFlow` 0x08 vs `DefaultWaterFlowBlend` 0x18) and must not be "fixed" by analogy with the undefined NIF-side `blend_normals` bit 16

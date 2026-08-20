# WATR-ARB-02: Rain and Displacement `Starting Size` are read from each other's block in three decoders

Filed: 2026-08-20 · Source: `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` · GitHub: #3105

- **Severity**: MEDIUM
- **Location**: `water.rs:524-531` (Oblivion), `:592-606` (FO3/FNV long `DATA`), `:803-807` + `:834-837` (Skyrim)
- **Status**: NEW
- **Evidence**: the per-game verdict table above; 17/17 Oblivion, 11/11 FO3 + 8/8 FNV long-`DATA`, 34/34 Skyrim.
- **Impact**: `mat.displacement[0]` and `mat.rain_start_size` are both live to the GPU (`byroredux/src/render/water.rs:300-303`). On Oblivion every water gets a ripple starting size 5× too small and a rain ripple 5× too large; on Skyrim `rain_start_size` is never set at all.
- **Fix**: read the displacement block as F/V/Fo/Dm/**Start** at `+0/+4/+8/+12/+16` from the displacement-force offset each decoder already uses (Oblivion 80, FO3/FNV/Skyrim 76) — i.e. `zip([96, 88, 92])` / `zip([92, 84, 88])` — and `rain_start_size` from rain-force `+16` (Oblivion 76, others 72). This matches the already-correct FO4 sibling.
---
*Filed from `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` — a byte-level arbitration run to resolve a direct contradiction between `/audit-fo4` and `/audit-legacy-compat` during the 2026-08-20 comprehensive suite. Authority is shipped bytes from all seven vanilla masters plus the GECK/CK default simulator tuple; `find / -iname "*Records.pas"` returns zero hits, so no xEdit definition exists on this machine.*

## Completeness Checks
- [ ] **SIBLING**: the same offset pattern checked in every other per-game WATR decoder in `crates/plugin/src/esm/records/misc/water.rs` (they do not share a helper)
- [ ] **CANONICAL-BOUNDARY**: per-game layout logic stays in the WATR decoder — never pushed into `resolve_water_material`, `render/water.rs` or `water.frag`. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins this fix against *shipped bytes*, not against the decoder's own output — three existing fixtures encode the current inversion and cannot catch it (see WATR-ARB-06)
- [ ] **NO-ANALOGY**: the WATR-side `FNAM` bit `0x10` decode is empirically correct (`DefaultWaterFlow` 0x08 vs `DefaultWaterFlowBlend` 0x18) and must not be "fixed" by analogy with the undefined NIF-side `blend_normals` bit 16

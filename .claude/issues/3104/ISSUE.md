# WATR-ARB-01: Skyrim `normal_magnitude` reads the Displacement Starting Size, collapsing every authored noise amplitude onto the shader floor

Filed: 2026-08-20 · Source: `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` · GitHub: #3104

- **Severity**: HIGH
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:834-837`; consumed at `byroredux/src/env_translate.rs:815-823`; reaches `crates/renderer/shaders/water.frag:690-716,313`
- **Status**: NEW (no matching issue in `/tmp/audit/issues.json`; the closest water issues — #2782/#2784/#2787/#2789/#2790/#2804/#2870/#2872/#2887/#2888/#2889 — are all renderer- or physics-side)
- **Evidence**: `DNAM[92] == 0.05` on 34/34 vanilla `Skyrim.esm` records (1 distinct value); the same offset holds `0.05` on 42/42 FO4 records where the file's own FO4 decoder and fixture correctly name it *Displacement Starting Size*; authored amplitudes at 184/188/192 span 0.0725–1.0 across 28+ distinct values.
- **Impact**: all 34 vanilla Skyrim water types render at the shader's minimum normal tilt with zero per-water differentiation. Skyrim is WATAL's canonical reference game.
- **Fix**: delete the `normal_magnitude ← DNAM[92]` assignment; leave the `1.0` sentinel until an offset is byte-decoded. Fix `displacement`/`rain_start_size` per WATR-ARB-02 in the same change.
---
*Filed from `docs/audits/AUDIT_WATR_ARBITRATION_2026-08-20.md` — a byte-level arbitration run to resolve a direct contradiction between `/audit-fo4` and `/audit-legacy-compat` during the 2026-08-20 comprehensive suite. Authority is shipped bytes from all seven vanilla masters plus the GECK/CK default simulator tuple; `find / -iname "*Records.pas"` returns zero hits, so no xEdit definition exists on this machine.*

## Completeness Checks
- [ ] **SIBLING**: the same offset pattern checked in every other per-game WATR decoder in `crates/plugin/src/esm/records/misc/water.rs` (they do not share a helper)
- [ ] **CANONICAL-BOUNDARY**: per-game layout logic stays in the WATR decoder — never pushed into `resolve_water_material`, `render/water.rs` or `water.frag`. See `/audit-nifal`.
- [ ] **TESTS**: a regression test pins this fix against *shipped bytes*, not against the decoder's own output — three existing fixtures encode the current inversion and cannot catch it (see WATR-ARB-06)
- [ ] **NO-ANALOGY**: the WATR-side `FNAM` bit `0x10` decode is empirically correct (`DefaultWaterFlow` 0x08 vs `DefaultWaterFlowBlend` 0x18) and must not be "fixed" by analogy with the undefined NIF-side `blend_normals` bit 16

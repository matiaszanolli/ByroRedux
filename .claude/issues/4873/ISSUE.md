# #4873: REN-D6-2026-09-24-04: material/NIFAL docs — bit-31 index selectors undocumented, nifal.md Phase-2 callers omit terrain_lod_btr.rs, ground-cover 'no Material' wording

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D6-2026-09-24-04**._

**Severity**: LOW · **Status**: NEW · every claim below was checked against the code by the auditor and re-checked against the tip of `main` at publish time.

**Dimension**: NIFAL Material

`shader-pipeline.md` presents `parallax_map_index`, `gloss_map_index` and `tint_map_index` as plain bindless indices although each carries a bit-31 channel selector every reader must mask (only `nifal.md:547` names one); `nifal.md`'s Phase-2 caller list omits `terrain_lod_btr.rs` (a Phase-2 caller of `resolve_msn_z_source` since #4632) and the terrain-exempt sentence lives only in a code comment; `translate_texture_only_material`'s doc says ground cover has "no `Material` at all", true only of blades since Phase C.

## Completeness Checks
- [ ] **SIBLING**: The same stale claim is checked in the neighbouring docs and code comments (doc moves with the pass)


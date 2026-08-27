# FNV-2026-08-26-D6-04

**Issue**: #3343
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 6 — Animation, Skinning & Particles
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/nif/tests/parse_real_nifs.rs:405-433`
(`real_archive_torch_meshes_surface_particle_emitters`)

**Premise verified**: the loop body's only assertion is
`if !emitters.is_empty() { total_emitters += emitters.len(); }` followed by
`if total_emitters > 0 { … return; }`. No decoded magnitude — rate, radius,
`base_scale`, life — is ever checked. **Not a duplicate of #3286**, which is
about the FO3 arm being structurally unreachable behind FNV's early `return`;
this is about what the arm that *does* run actually asserts. The FNV arm runs
and passes today (`[Fallout New Vegas] 182 emitters across 5 meshes`) while
D6-03 above is live.

**Evidence**: `cargo test -p byroredux-nif` is fully green at HEAD, and so is
this test on real FNV data, yet `_tmp_fo3_emitter_survey` measures
`with_emitter=307 rate_some=100` on the same archive.

**Impact**: the one piece of real-archive infrastructure that could pin the
typed-emitter decode (`5708b5b9` / `9db60714`) cannot detect a regression that
zeroes every authored rate, size or `base_scale` — only one that removes the
emitter blocks entirely.

**Fix sketch**: alongside the #3286 per-game accumulation fix, assert per game
that a floor fraction of emitters carry `emitter_rate.is_some()` and
`emitter_params.is_some()` with finite positive `initial_radius`/`life_span`,
checked-in as a baseline (the FNV numbers today are 307/307 params and
100/307 rate — pin them so both directions move deliberately).

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

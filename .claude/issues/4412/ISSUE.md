# #4412 — NIFAL-D9-2026-09-14-03: Skill / audit-protocol / harness doc-rot that misdirects future audits of this layer (bundle)

**Labels**: low,nifal,documentation,doc-rot,tech-debt
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (doc)
- **Dimension**: Completeness
- **Tier Violated**: harness-coverage gap (documentation audits rely on)
- **Game Affected**: all
- **Location / items**: each verified against the live tree. The two `byroredux/src/cell_loader/object_lod.rs` caller/exemption items are carried by NIFAL-D1-2026-09-14-04 / #4246 and are not repeated here.
  1. `.claude/commands/audit-nifal/SKILL.md:186`: particle call-site hints "~line 513" / "~line 642". The live sites are `byroredux/src/scene/nif_loader.rs:1539` and `byroredux/src/cell_loader/spawn.rs:1191`.
  2. `.claude/commands/audit-nifal/SKILL.md:210`: "carrying only the three `u32` counts". `CollisionAuthoringSummary` (`crates/nif/src/import/collision/mod.rs:89-102`) now has four (`plane_shapes`, #4163). An auditor applying the invariant literally would flag the fourth count as a leak.
  3. `.claude/commands/_audit-common.md:208`: Skyrim LE "No BYROREDUX_* env var reads this path yet". Since `fb8173fe0`, `BYROREDUX_SKYRIMLE_DATA` is read (`crates/nif/tests/common/mod.rs:89`).
  4. `crates/nif/tests/translation_completeness.rs:341`: the `#[ignore]` reason omits SkyrimLE, and the module doc at `:40` still says "default Steam install paths" (LE's fallback is a Wine prefix).
  5. `.claude/commands/audit-nifal/SKILL.md:258`: describes the harness as a fill rate "over the canonical `Material` slots". It measures the raw pre-merge `ImportedMaterial` tier (#2214), and that misreading is what makes FO76/Starfield near-zero `tex`/`nrm` look like leaks.
- **Status**: NEW (the `.claude/commands/audit-nifal/SKILL.md:252` "#3814 still-open" item is Existing: #4369 and excluded)
- **Description / Impact**: Each item points a future Dim 5/6/9 auditor at the wrong line, count or tier. Item 2 would manufacture a false no-leak finding; item 5 invites re-filing documented structural zeros.
- **Related**: #4369, #4246, #4360, NIFAL-D1-2026-09-14-04, NIFAL-D6-2026-09-14-01 (which also needs a skill-wording fix).
- **Suggested Fix**: Update the five locations. For the caller list, consider a doc-scan test in the style of `documented_texture_role_list_matches_the_struct` that derives the `translate_material(` caller set from `byroredux/src/`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

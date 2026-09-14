# #4407 — NIFAL-D6-2026-09-14-01: `BhkPlaneShape → None` is justified by a trimesh fallback that never fires for its only vanilla instance

**Labels**: low,nifal,physics,bug,game:skyrim
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (one small underwater egg-cluster file; the deliberate `None` stays sound — only the documented safety net is false). The orchestrator considered the MEDIUM "silently dropped collision shape" row and rejected it: the drop is deliberate and documented at its arm, not silent.
- **Dimension**: Collision
- **Tier Violated**: parked-not-leak (the parked `None` is described as covered downstream, but nothing covers it)
- **Game Affected**: Skyrim SE
- **Location**: `crates/nif/src/import/collision/shape.rs:93-104` (claim), `byroredux/src/cell_loader/spawn/mesh_instance.rs:1318-1325` (gate that rejects the fallback)
- **Status**: NEW (related closed #1334, #4163)
- **Description**: The arm's comment says the dropped plane falls back to "the synthesized-trimesh fallback (spawn.rs) — its render-mesh surface". The one vanilla file, `slaughterfisheggcluster01_1.nif` (`Skyrim - Meshes1.bsa`), has the plane as its only collision and one `BSTriShape` with `NiAlphaProperty` flags `0x12EC`, so `alpha_test = true`. The trimesh fallback requires `!source_material.alpha_test`, so the placement gets no collider. #4163's `plane_shapes` counter has no production reader: it is not in `collision_authoring_totals` or `SpawnCensusAuthoring`, so the drop is invisible at runtime.
- **Evidence**: The orchestrator re-read `crates/nif/src/import/collision/shape.rs:93-104` and `byroredux/src/cell_loader/spawn/mesh_instance.rs:1318-1325`. The agent's `trace_block` probe of the file shows the block list above.
- **Impact**: One cosmetic physics-only collider missing; the documented-limitation rule rests on an unmeasured claim.
- **Related**: #1334, #4163, #2355.
- **Suggested Fix**: Correct the comment, `.claude/commands/audit-nifal/SKILL.md` and nifal.md wording to "no collider is produced for this instance". Optionally fold `plane_shapes` into `collision_authoring_totals` / `SpawnCensusAuthoring`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

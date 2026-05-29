# #1306 -- OBL-D6-NEW-03: Oblivion FX emitters drop from render

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: MEDIUM | **Dim 6** — Blockers & Game-Specific Quirks
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D6-NEW-03)

**Location**: `crates/nif/src/blocks/particle.rs` (NiPSysData + modifier parsers) → `crates/nif/src/blocks/mod.rs:927-939`; render-side at `byroredux/src/cell_loader/references.rs:930` → `byroredux/src/cell_loader/spawn.rs:378`

**Issue**: Oblivion FX content (fire/torch/brazier flames, smoke columns, ambient fireflies, magic-effect meshes) parses with truncation — the NiPSys emitter is lost before `import_nif_particle_emitters` can capture it. The exact 4-byte under-read that drifts the stream entering NiPSysData on v20.0.0.4/5 is not yet pinned (the `NiPSysAgeDeathModifier` boundary is the suspected site). When the emitter is dropped, `spawn.rs:378` silently skips the FX entity.

**Suggested fix**: pin the exact NiPSysData trailing field that under-reads at v20.0.0.4/v20.0.0.5 (trace the drift at the NiPSysData→NiPSysAgeDeathModifier boundary). Add a regression test on a real Oblivion FX NIF (e.g. a torch from `Oblivion - Meshes.bsa`).

## Completeness Checks
- [ ] **SIBLING**: check FO3 NiPSysData layout — FO3 also ships v20.2.0.7 NiPSys content on the same parser
- [ ] **TESTS**: regression test on an Oblivion FX NIF asserting the emitter is captured
- [ ] **CANONICAL-BOUNDARY**: parse-side; particle NIFAL (typed emitter blocks + apply_emitter_params) already wired
- [ ] **UNSAFE**: no unsafe involved

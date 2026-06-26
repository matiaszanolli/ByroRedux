# REN-2026-06-26-I01: audit-renderer Dim-12 checklist mis-describes #1235 (NIF-root-flags parity, not world-resource vs cached-snapshot)

**Audit**: `docs/audits/AUDIT_RENDERER_2026-06-26.md` (Dimension 12 — command-buffer recording)
**Severity**: INFO (audit-skill checklist drift; no engine code defect)
**Status**: NEW, CONFIRMED against live `.claude/commands/audit-renderer/SKILL.md`

## Description
The Dimension-12 checklist in `.claude/commands/audit-renderer/SKILL.md` reads:

> cell-loader REFR spawn reads `SceneFlags` from the world resource, not a cached snapshot (#1235)

That description does not match what #1235 (LC-D1-NEW-01) actually does. The real guard
attaches a **per-entity** `SceneFlags` ECS component on the placement root, derived from the
NIF root `NiAVObject.flags` (`cached.root_flags`), for parity with the loose-NIF loader. There
is no "cached snapshot vs world resource" divergence anywhere in the path. The underlying guard
holds — only the checklist prose is wrong, and it would send a future auditor looking for the
wrong mechanism.

## Evidence
- `.claude/commands/audit-renderer/SKILL.md` line ~225 (Dimension 12, "Counter independence" bullet).
- Live code: `byroredux/src/cell_loader/spawn.rs` — `if cached.root_flags != 0 { world.insert(placement_root, SceneFlags::from_nif(cached.root_flags)); }`.
- `byroredux/src/cell_loader/nif_import_registry.rs` doc confirms `root_flags` is captured at import time for the spawn-site `SceneFlags` insert. APP_CULLED (bit 0) is filtered import-side in `crates/nif/src/import/walk/mod.rs`; the remaining bits ride through.

## Impact
None functional. Pure audit-infrastructure doc-rot — misdirects the next renderer audit.

## Suggested Fix
Reword the Dim-12 checklist bullet to:
> cell-loader REFR spawn attaches a per-entity `SceneFlags` from the NIF root `NiAVObject.flags` for parity with the loose-NIF loader (#1235).

No engine code change.

## Completeness Checks
- [ ] **SIBLING**: confirm no other audit skill repeats the same #1235 mis-description
- [ ] **DOC**: the reworded bullet still references #1235 and the correct symbol (`SceneFlags::from_nif`)

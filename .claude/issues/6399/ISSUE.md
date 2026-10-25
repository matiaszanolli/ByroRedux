# TD8-2026-10-25-06: `LightKind` is a "Backward-compatible name" alias with ~25 production uses

Labels: low, ecs, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6399
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Effort**: small · **Status**: NEW
- **Location**: `crates/core/src/ecs/components/light.rs:6-7` `pub type LightKind = EmitterKind;` Consumers still spell the alias: `byroredux/src/render/lights.rs:15,80-84`, `cell_loader/spawn.rs:648`, `spawn/mesh_instance.rs:572`, `spawn/{authored_cover,precombined}.rs`, `precombined.rs:500`, `cornell/builders.rs:211` (76 whole-repo hits incl. tests vs 12 for `EmitterKind`). The canonical name is `EmitterKind`; the engine has no external consumers, so "backward-compatible" protects nothing and leaves two names for one type.
- **Fix**: `sed` the alias to `EmitterKind` across `byroredux/` + `crates/core`, delete the alias and its re-export.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD8-2026-10-25-06`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

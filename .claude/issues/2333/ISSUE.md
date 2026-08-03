# FO3-D5-03: examine_collision_kind/CollisionAuthoring have no callers — doc comment names consumers that don't exist

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2333

**Severity**: LOW
**Location**: `crates/nif/src/import/collision/mod.rs:40-85`
**Status**: NEW

### Description
The doc claims the discriminator exists so callers (telemetry, the trimesh fallback in `cell_loader/spawn.rs`) can distinguish authoring kinds. A repo-wide search finds both symbols referenced only inside their own module and its tests — `spawn.rs` never calls either, and its NP-fallback comment refers to `extract_collision` instead.

Confirmed against current code: a grep for `examine_collision_kind`/`CollisionAuthoring` across `crates/nif/src/` and `byroredux/src/`, excluding the defining module, returns zero hits — no external caller exists anywhere in the codebase.

### Impact
Dead public API plus doc drift that would mislead a future reader into believing FO3/FO4 collision telemetry is wired up.

### Suggested Fix
Wire it into the `spawn.rs` fallback, or delete it and correct the docstring.

### Related
FO3-D5-02

## Completeness Checks
- [ ] **TESTS**: If wired into `spawn.rs`, a regression test pins the new fallback-selection behavior; if deleted, no test needed beyond compile-clean

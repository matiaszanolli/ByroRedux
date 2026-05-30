# #1322 -- TD-D8: Dead re-export LegacyFormId/LegacyLoadOrder

_Snapshot as filed 2026-05-29 from /audit-publish (AUDIT_TECH_DEBT_2026-05-28)._

**Severity**: LOW | **Dim 8** — Backwards-Compat Cruft / API Surface
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-05-28.md` (TD8-D8-NEW-01)
**Domain**: nif-parser | **Effort**: trivial

**Location**: `crates/plugin/src/legacy/mod.rs` (or `esm/legacy.rs`)

**Issue**: `pub use legacy::{LegacyFormId, LegacyLoadOrder}` re-exports two types that have zero external callers in the workspace. The types were kept as a "compatibility" shim after the per-game legacy stubs were removed under #390. Since ByroRedux has no external consumers yet, this is pure rot — it creates a public surface that future tooling (e.g. cargo-semver-checks) would lock in.

**Fix**: Remove the re-export (and the types themselves if they have no internal callers). If the types are still needed internally, change visibility to `pub(crate)`.

## Completeness Checks
- [ ] **SIBLING**: check `crates/plugin/src/lib.rs` for other dead re-exports from legacy modules
- [ ] **TESTS**: `cargo check --workspace` confirms; no behavior change
- [ ] **CANONICAL-BOUNDARY**: N/A (plugin/ESM crate, no material path)
- [ ] **UNSAFE**: no unsafe

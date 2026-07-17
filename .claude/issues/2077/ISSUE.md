# TD8-103: npc_spawn.rs's two pub use re-exports claim existing call sites that don't exist

**GitHub Issue**: #2077
**Labels**: low,tech-debt,bug

**Severity**: LOW
**Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
**Location**: `byroredux/src/npc_spawn.rs:29-33,431-435`

## Description
Both re-exports (`Gender`, `normalize_mesh_path`) carry comments justifying `pub use` for "existing call sites" that don't exist anywhere in the tree; `byroredux` is a single binary crate with no external consumers by definition.

## Evidence
Confirmed live: `byroredux/src/npc_spawn.rs:29-33` — `pub use byroredux_plugin::equip::Gender;` with comment "Re-exported here so existing call sites continue to use `npc_spawn::Gender`." `byroredux/src/npc_spawn.rs:431-435` — `pub use crate::asset_provider::normalize_mesh_path;` with comment "Re-export keeps the existing call sites here compiling." Grepped the full tree for `npc_spawn::Gender` and `npc_spawn::normalize_mesh_path` (and any `use ... npc_spawn::{...Gender...}` / `...normalize_mesh_path...}` imports) — zero matches outside `npc_spawn.rs` itself.

## Suggested Fix
Change both to plain `use`; delete the misleading comments.

**Effort**: trivial

## Completeness Checks
- [ ] **TESTS**: N/A (visibility-only change) — `cargo check` confirms no external call site breaks

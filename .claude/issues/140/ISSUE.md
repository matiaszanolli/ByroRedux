# Issue #140 (E26-04): Query/resource method docs don't mention lock_tracker deadlock panics

**Severity**: LOW | **Dimension**: Documentation | **Domain**: ecs
**Audit**: AUDIT_ECS_2026-04-07.md

**Location**: `crates/core/src/ecs/world.rs` — 12 methods

**Fix**: Add `# Panics (debug only)` section to each method's doc comment noting lock_tracker panics.

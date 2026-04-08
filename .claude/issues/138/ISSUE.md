# Issue #138 (E26-02): has() and count() use manual track/untrack instead of RAII guard

**Severity**: LOW | **Dimension**: Query Safety (hardening) | **Domain**: ecs
**Audit**: AUDIT_ECS_2026-04-07.md

**Location**: `crates/core/src/ecs/world.rs:137-166`

**Fix**: Rewrite `has()` / `count()` to use `self.query::<T>()` and the QueryRead API.

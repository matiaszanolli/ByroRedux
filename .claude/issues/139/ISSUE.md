# Issue #139 (E26-03): resource.rs — Drop impl for ResourceWrite appears before struct definition

**Severity**: LOW | **Dimension**: Code Quality | **Domain**: ecs
**Audit**: AUDIT_ECS_2026-04-07.md

**Location**: `crates/core/src/ecs/resource.rs:44-48` vs `61-65`

**Fix**: Move `Drop for ResourceWrite` impl to after the `ResourceWrite` struct definition.

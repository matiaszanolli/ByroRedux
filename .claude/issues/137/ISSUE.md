# Issue #137 (E26-01): lock_tracker state leaks if lock.read/write() panics after track_*

**Severity**: LOW | **Dimension**: Query Safety (hardening) | **Domain**: ecs
**Audit**: AUDIT_ECS_2026-04-07.md

**Location**: `crates/core/src/ecs/world.rs` — 9 methods

**Fix**: RAII scope guard that untracks on drop, then `mem::forget` on success path.

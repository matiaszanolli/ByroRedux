# PHYS-D3-06

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2881

---

Found by `/audit-physics` Dimension 3 (ECS Sync). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/physics/src/sync.rs:104`, `:170-173`

## Trigger Conditions
Every frame, unconditionally.

## Description
`physics_sync_system` calls `std::env::var_os("BYRO_PROFILE")` at `:104` and `std::env::var_os("BYRO_PROFILE_FALLERS")` at `:171` on every tick. On Linux each takes the process-wide environ lock and allocates an `OsString` on a hit. The comment at `:170` claims *"Zero cost when the flag is unset"*, which is inaccurate — the lookup itself is the cost, not the branch.

## Evidence
```rust
// sync.rs:104
let profile = std::env::var_os("BYRO_PROFILE").is_some();
// sync.rs:171
if std::env::var_os("BYRO_PROFILE_FALLERS").is_some() {
```

## Impact
Negligible in absolute terms (2 lookups/frame). Reported only because the adjacent comment asserts a property the code does not have, and the same pattern is copied elsewhere in the crate.

## Suggested Fix
Hoist both into a `std::sync::OnceLock<bool>` (or `LazyLock`) read once per process, and correct the comment.

# #204: TS-01: Lock tracker compiles to no-ops in release builds
- **Severity**: MEDIUM — **Domain**: ecs — **Dimension**: Thread Safety
- **Location**: `crates/core/src/ecs/lock_tracker.rs:202-246`
- **Fix**: Keep tracker active in release or use try_write() with timeout

# RT-2026-10-31-11: `help` (and any read-reentrant command) trips the recursive-read WARN even though the contract permits it

Labels: low,ecs,tech-debt,bug
Source: docs/audits/AUDIT_RUNTIME_2026-10-31.md

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-31.md` RT-2026-10-31-11 (report HEAD `670f81917`).
**Re-verified at HEAD `7f2d98944`**: `byroredux/src/commands/world_info.rs:22` and `lock_tracker.rs` are unchanged since the report.

- **Severity**: LOW
- **Dimension**: Diagnostics noise
- **Location**: `byroredux/src/commands/world_info.rs:22` (`HelpCommand::execute` re-reads `CommandRegistry`, documented as permitted by CONC-D3-04 / #1786), `crates/core/src/ecs/lock_tracker.rs`
- **Status**: NEW
- **Description**: Running `help` over `byro-dbg` logs `ECS recursive-read hazard: … CommandRegistry … acquired at world_info.rs:22:30`. The source comment says read-read re-entry is permitted by the tracker. Because the tracker warns once per type per thread, a benign instance can mask the first genuine one on the same thread (RT-2026-10-31-08 is that kind of case).
- **Suggested Fix**: Allow-list `CommandRegistry` in the tracker, or pass the registry down into `execute`.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Same pattern checked in related call sites
- [ ] **TESTS**: A regression test pins this specific fix

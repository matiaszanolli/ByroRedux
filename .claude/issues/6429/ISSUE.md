# CHAR-2026-10-26-L-08: `vats_mode_system` declares 7 accesses; its body (via `vats_scan`) touches a dozen more

- **Severity**: LOW
- **Dimension**: Progression & Pools (rule owned by `/audit-concurrency` Dim 4)
- **Game**: fo3, fnv
- **Location**: `byroredux/src/boot/schedule/early.rs:110-123` vs `byroredux/src/systems/vats.rs` (`vats_scan`: `LoadedCellIndex`, `ActorValues`, `ActorVitals`, `FormIdComponent`, `GlobalTransform`, `CreatureActor`, `EquippedWeapon`, `FormIdPool`, `StringPool`, `SubtreeCache`, `PhysicsWorld`, `RapierHandles`, `ActorColliderOwner`, camera)
- **Status**: NEW
- **Description**: The system is exclusive, so there is no race. The declaration exists only for the `sys.accesses` diagnostics report, and it understates the body. `pool_regen_tick_system`'s declaration matches its body exactly.
- **Suggested Fix**: Add the missing reads, or document that the declaration lists only the mode switch.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-08`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **LOCK_ORDER**: If a scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)


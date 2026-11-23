# #7303: SCR-D3-04: cleanup.rs's Pattern-B table and summary paragraph still describe the pre-#6150 drain shapes

- **Severity**: LOW (doc rot)
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/cleanup.rs:47-48,57`; the Dim 3 text of `.claude/commands/audit-scripting/SKILL.md` (the `HavokImpulseRequest` sentence)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The `MotionTypeChangeRequest` row says "the one tail-drain — it removes exactly the entities it snapshotted, after an empty-set early return". At HEAD `scripted_motion_type_system` opens with `let asked = super::take_requests::<MotionTypeChangeRequest>(world);` (`systems/cinematic.rs:417`), a head drain, and `take_requests` is used identically by all eight binary consumers (`systems.rs:113`). The `HavokImpulseRequest` row's "snapshots every request and drains exactly those; … a missing HavokImpulseRules drains the snapshot before returning" is likewise now by construction. The behaviour is correct; the table misleads a future author about which drain shape is canonical.
- **Related**: #6150 (the `take_requests` extraction)
- **Suggested Fix**: Rewrite the rows as "head drain via `take_requests` (#6150)". Drop the tail-drain wording.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix

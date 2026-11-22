# #7254: INC-2026-11-22-13: #7225 left the same defect for Near Editor Location Guard (and the other anchor-from-current packages)

**Labels**: low,gameplay,ai,bug

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-C-04**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: LOW
- **Dimension**: gameplay Dim 5 (SIBLING check)
- **Location**: `byroredux/src/systems/guard.rs:89-90,203-211` (`resolve_anchor(.., home = current)`); `byroredux/src/npc_spawn/ai_package.rs:487-492` (`Guard { anchor_form_id: location_reference, .. }`, no PLDT type carried)
- **Changed in**: not touched by a0e45a578 (#7225)
- **Status**: NEW (sibling of closed #7225; the issue's own Suggested Fix said "and Patrol/Guard/Sandbox as needed")
- **Description**: `GuardBehavior` carries no location type. A Guard package whose PLDT is Near Editor Location (type 3) resolves its anchor to the actor's position the first time `guard_system` sees it, i.e. where an earlier package left the actor, exactly the #7225 pattern; the GECK page for Guard calls the location "where the actor stands while running the guard package". Sandbox and Patrol take their centre from the same kind of "current position" fallback.
- **Evidence**: `resolve_anchor` returns `home` (`current`) when `anchor_form_id` does not resolve; `from_package`'s Guard arm never inspects `location_anchor(package.location)`.
- **Impact**: Unmeasured. A guard handed its post by a package after being moved holds the wrong spot. I did not count vanilla Guard packages with type 3, so frequency is unverified.
- **Related**: #7225 (closed), #5391
- **Suggested Fix**: Carry `near_editor_location` on `GuardBehavior` and anchor on `EditorPlacement` as `travel_home` / the wander home now do; count vanilla Guard/Sandbox/Patrol type-3 packages first to rank it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

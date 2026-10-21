# #6327: PHYS-D3-2026-10-21-03: The #6070 stand-in-template guard scans a whole-file window that contains its own needles

**Labels**: low,bug,physics,test-gap

**Source**: docs/audits/AUDIT_PHYSICS_2026-10-21.md · **Severity**: LOW · **Dimension**: Ragdoll & Constraint Seam (guard hygiene) · **Location**: `byroredux/src/npc_spawn/resumable/mod.rs:1166-1180` (`the_stand_in_ragdoll_template_never_replaces_a_decoded_one`)

- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam (guard hygiene)
- **Location**: `byroredux/src/npc_spawn/resumable/mod.rs:1166-1180` (`the_stand_in_ragdoll_template_never_replaces_a_decoded_one`)
- **Status**: NEW
- **Description**: The test takes `include_str!("mod.rs")` from `fn spawn_skeleton_unit(` to the end of the file. That window includes the test module and the test's own two needle literals, which the source-scan rule (#4604 / #4842 / #5164; `_audit-common.md` Methodology) forbids. If the production install call is renamed or reformatted while the guard text survives elsewhere in production, `find` matches the test's literal for the install, and `guard < install` passes vacuously. An ordering check also does not show that the guard wraps the call.
- **Evidence**: `resumable/mod.rs:1168-1179`. `no_source_scan_cuts_with_the_weak_cfg_test_needle` does not catch it, because the test makes no cut at all.
- **Impact**: The #6070 regression guard can go blind silently. The behaviour itself is also covered indirectly by the live smoke, but not in `cargo test`.
- **Suggested Fix**: Scan `byroredux_core::source_scan::production_text(include_str!("mod.rs"))`. Better: replace it with a behavioural test that runs `spawn_skeleton_unit`'s template step on a skeleton root that already holds a decoded `RagdollTemplate`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_PHYSICS_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._

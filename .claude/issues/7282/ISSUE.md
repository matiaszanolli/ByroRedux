# #7282: PHYS-D3-01: The tilted-corpse gap cites the closed #6070, not its open tracker #6677

- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam (doc rot)
- **Location**: `byroredux/src/ragdoll_pose.rs:96`; `docs/engine/physal.md:447`; `.claude/commands/audit-physics/SKILL.md` (Known-open register, "Open with no issue of its own"); `crates/nif/src/import/collision/np_ragdoll.rs:324` (`decode_np_ragdoll` doc)
- **Status**: NEW
- **Description**: Several places are stale:
  - The module doc heads the gap "Placement tilt (#6070, open)". #6070 is closed (it was the FO4 ragdoll smoke-test issue).
  - The gap is tracked by #6677 (OPEN: "FO4 tilted authored corpses … burst on load").
  - `physal.md` and the audit skill's register say the gap has no issue of its own.
  - `decode_np_ragdoll`'s doc does not mention the #7133 refusal (a packfile that is not 64-bit Havok 2014 returns `None`).
- **Evidence**: `//! **Placement tilt (#6070, open).**`; `gh issue view 6070` → CLOSED; `gh issue view 6677` → OPEN.
- **Trigger Conditions**: n/a (documentation)
- **Impact**: A reader following the citation lands on a closed smoke-test issue. The audit register invites a duplicate filing.
- **Related**: #6677, #6070, #7133
- **Suggested Fix**: Cite #6677 at all three sites, and add the #7133 refusal to `decode_np_ragdoll`'s doc.

**Found by**: `docs/audits/AUDIT_PHYSICS_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: every #6070 citation checked

# SCR-D5-2026-10-17-08: `scripted_havok_impulse_system` can strand `HavokImpulseRequest` markers; the documented Pattern-B contract is inaccurate

Issue: #6009
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D5-2026-10-17-08: `scripted_havok_impulse_system` can strand `HavokImpulseRequest` markers; the documented Pattern-B contract is inaccurate

- **Severity**: LOW
- **Dimension**: Scene/Package/Cinematic
- **Location**: `byroredux/src/systems/cinematic.rs:349-351` (early `return` when `HavokImpulseRules` is absent, before the tail drain at :379-383); contract text `crates/scripting/src/cleanup.rs:45` ("early-returns only when there are none")
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The marker table promises the system snapshots, drains exactly those, and early-returns only on an empty set. After the body has resolved `reference_body_entities` for every request, a missing `HavokImpulseRules` returns without draining, so the requests sit and are re-resolved (an O(requests x rigid bodies) scan) every frame until the resource returns. The native queues only when the resource exists (reference.rs:200), so reachability is a load-order change inside one frame; the defect is the contract rot plus the latent strand.
- **Suggested Fix**: drain `asked` before the rules check (or check the resource first), and correct the cleanup.rs table row.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix

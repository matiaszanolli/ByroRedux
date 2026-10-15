# #5869: GAME-D5-2026-10-15-02: The Travel-arrival End fragment (`7d34b99f2`) is unreachable on fragment-bearing games

- **Labels**: low,bug,gameplay,ai,scripting
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `byroredux/src/systems/travel.rs:369-380`; tree fallback `byroredux/src/npc_spawn/ai_package.rs:349-389`
- **Status**: NEW
- **Description**: `TravelBehavior` installs only from the flat `procedure_type == 6` arm, which only FO3/FNV/Oblivion packages reach. Skyrim+ packages use the procedure tree, which resolves Sandbox and Patrol leaves only (`docs/engine/packal.md`), and PACK VMAD exists only on Skyrim+. So every End this path reports belongs to a package without fragments. Only the synthetic VM test drives it, yet `scripting.md` and the commit message describe it as live. Escort, Guard, Eat and Sleep report no arrival End at all.
- **Suggested Fix**: Correct the doc, or resolve the Travel leaf on tree packages first (`packal.md` scope).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix

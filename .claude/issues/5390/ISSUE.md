# #5390: GAME-D5-2026-10-08-02: Sleep can never reach a sleep marker on FO3/FNV — every sleeper sits upright in the nearest sit marker (chair or bed), and Eat can pick bed markers

**Labels**: medium,gameplay,ai,bug,game:fnv,game:fo3
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5390

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D5-2026-10-08-02` (HEAD `00f580e09`)

- **Severity**: MEDIUM
- **Dimension**: Dim 5
- **Location**: `byroredux/src/systems/eat_sleep.rs:173-180, 225-227`; `byroredux/src/cell_loader/references/attach.rs:57-71`
- **Status**: NEW
- **Trigger**: FNV/FO3, any NPC whose Sleep package wins (617 NPC-default references in FalloutNV.esm), in a cell with furniture, while `SandboxSitClip` resolves.
- **Description**: `seat_at_marker` prefers markers whose `kind == FurnitureMarkerKind::Sleep` and falls back to `is_sit_marker`. The translate boundary resolves `Sleep` only from `animation_type == 2`, a Skyrim+ field. Legacy Oblivion/FO3/FNV markers carry `animation_type = 0` and always resolve to `Sit`, a documented over-match that includes bed and lean markers.

  PKDT procedures 3 and 4 exist only in the FO3/FNV enum, and Skyrim tree packages resolve only Sandbox and Patrol leaves. So in production the sleep-marker preference never matches. Every sleeper takes the nearest `Sit` marker, chairs included, and an Eat actor can equally take a bed's marker. The unit test `sleep_actor_prefers_the_sleep_marker` passes only because it hand-builds an `animation_type: 2` marker that FO3/FNV data cannot produce. The module doc's "falling back to sit markers when a cell's beds author none" therefore describes every FO3/FNV bed.
- **Evidence**: `kind: match m.animation_type { 2 => Sleep, 3 => Lean, _ => Sit }` (attach.rs). The FO3/FNV comment says: "0 = legacy … no AnimationType authored at all".
- **Impact**: NPCs "sleep" sitting in chairs all night, and diners sit on beds. The Sleep/Eat distinction is silently absent on the only games that run these procedures.
- **Related**: `FurnitureMarkerKind` Phase C (legacy `furnituremarkerNN.nif` decode deferred).
- **Suggested Fix**: Until legacy marker kinds are decoded, make the Sleep arm fall back explicitly and document it. Better, resolve legacy bed furniture from the FURN record or the marker's referenced `furnituremarkerNN.nif` at the translate boundary, so `Sleep` is reachable.

## Completeness Checks
- [ ] **SIBLING**: Eat arm and sandbox seat selection checked for the same legacy-kind over-match
- [ ] **TESTS**: A regression test pins this specific fix

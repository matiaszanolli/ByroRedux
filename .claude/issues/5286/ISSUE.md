# #5286 — GAME-D6-2026-10-05-01: drain_transition_notifications documents that it tolerates an absent QuestDefinitionRegistry, then .expects it

- **Labels**: low,gameplay,quests,ui,bug
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5286

- **Severity**: LOW (the panic is unreachable today; the contract and the code disagree)
- **Dimension**: 6 — Player feedback (#5153 journal announcements)
- **Location**: `byroredux/src/objectives.rs:175-214`, specifically `:187-190` and `:198`. The function is called every frame from `app_frame.rs:254`.
- **Status**: NEW
- **Description**:
  - The function's comment says: "The definitions guard is optional: a notification must compose even when the registry is absent (the objective state outlived it across a reload boundary) — the objective then announces by index".
  - The objective *text* honours that: `definitions.as_ref().and_then(...)`. The quest *name* does not: `quest_display_name(definitions.as_ref().expect("guard above"), event.quest)`.
  - There is no guard above. With pending objective events and no registry, the per-frame drain panics in the render loop.
  - Production installs the registry at scripting registration (`crates/scripting/src/lib.rs:209` → `quest_stages::register`) and never removes it, so this is latent.
- **Suggested Fix**: fall back to `format!("Quest 0x{:08X}", quest.0)` when `definitions` is `None` (for example, make `quest_display_name` take an `Option`). Add a test that drains an event with no registry.

_Source: `AUDIT_GAMEPLAY_2026-10-05.md` (GAME-D6-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix

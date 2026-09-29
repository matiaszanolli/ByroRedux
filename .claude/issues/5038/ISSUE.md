# #5038 — ECS-2026-09-29-D7-01: Dialogue surface reads a never-cleared per-NPC NpcDialogueTopic as a singleton — wrong NPC's topic after a second conversation

**Labels**: medium, bug, ecs, gameplay, dialogue

**Source**: `docs/audits/AUDIT_ECS_2026-09-29.md` — finding `ECS-2026-09-29-D7-01`

**Severity**: MEDIUM (incorrect gameplay/UI behavior). Owner overlap: `/audit-gameplay` (dialogue) and `/audit-ui`.

**Dimension**: 7 — Component Lifecycles

**Location**:
- `byroredux/src/systems/npc_dialogue.rs:305-310` (`dialogue_snapshot`)
- `byroredux/src/systems/npc_dialogue.rs:144-156` (`apply_selection`)

**Status in report**: NEW

## Description

- `apply_selection` inserts or overwrites `NpcDialogueTopic` on the selected NPC, and nothing removes it except `despawn`. The type's doc says "one per activation, overwritten by the next", but that is only true per NPC.
- `dialogue_snapshot` builds the native response surface from `world.query::<NpcDialogueTopic>()…iter().next()`. It ignores `DialogueSurfaceState.npc`, which `apply_selection` sets to the NPC that was just selected.

Talk to NPC A, then NPC B, in the same loaded area. Both now carry the component, and sparse dense order (insertion order) returns A. The surface opens on B's serial bump but presents A's topic, response text and topic list. The snapshot's `npc: A` then routes the next `SelectTopic` click to A.

## Evidence

```rust
let (npc, topic) = world
    .query::<NpcDialogueTopic>()
    .and_then(|topics| topics.iter().next().map(|(npc, topic)| (npc, topic.clone())))?;
```
The `766e1746e` tests cover a single NPC (re-selection + ownership refusal + serial bumps), so they cannot observe this.

## Impact

From the second conversation in a cell onward, the dialogue UI shows and acts on the wrong NPC until that NPC's cell unloads.

## Related

D5-01 (the same system's Access row).

Dialogue-landing cluster (`ab31cfefe` / `766e1746e`), filed as distinct issues: ECS-2026-09-29-D1-01 (#5025), ECS-2026-09-29-D1-02 (#5032), ECS-2026-09-29-D5-01 (#5035), SCR-D3-2026-09-29-01 (#5041), LC-D3-01 (#5045), ESM-2026-09-29-D2-03 (#5048). The per-frame Talk-candidate scan on the same code (ECS-2026-09-29-D6-01 = PERF-D1-2026-09-29-01) is tracked under open #3475.

## Suggested Fix

- Key the snapshot on `DialogueSurfaceState.npc` (`world.get::<NpcDialogueTopic>(npc)`).
- Clear the previous NPC's `NpcDialogueTopic` on a new selection or on surface close, so the component matches its documented one-live-selection posture.
- Add a two-NPC test.

Validated at HEAD 9fcfdc3fc: `dialogue_snapshot` (`byroredux/src/systems/npc_dialogue.rs`) still takes `query::<NpcDialogueTopic>()…iter().next()` and ignores `DialogueSurfaceState.npc`; `apply_selection` inserts `NpcDialogueTopic` and no non-despawn path removes it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other debug-UI snapshot readers that take `.iter().next()` of a per-entity component)
- [ ] **TESTS**: a two-NPC test (talk to A, then B → surface shows B)

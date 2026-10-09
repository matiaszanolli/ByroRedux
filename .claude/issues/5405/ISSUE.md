# #5405: SCR-D5-2026-10-08-03: `build_story_manager_tree` follows a single `SNAM` chain per sibling group, so 41 Skyrim quest nodes on detached chain segments are never evaluated. Among them are the Companions radiant givers and five bounty holds

**Labels**: medium,scripting,quests,bug,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5405

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-10-08.md` — `SCR-D5-2026-10-08-03` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `build_story_manager_tree` picks one un-targeted head per parent (`story_manager.rs` ~322-338) and the walk follows `next_sibling` only from it.

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Dialogue (Story Manager)
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/story_manager.rs:322-338`: the head is the first member no sibling points at, and only that
    one is stored as `first_child`.
  - `:631-640`: the walk follows `next_sibling` from that head only.
- **Status**: NEW. The design doc's "441/443 sibling integrity" check verifies only that a target shares the parent. It
  never checks that each group forms a single chain.
- **Description**: some vanilla groups do not form one linked list in either `SNAM` reading. They have several
  `SNAM == 0` members and several un-targeted members. The builder picks one head and drops every other segment
  without a warning.
- **Evidence** (Skyrim.esm; members reached by the code's walk / total):
  - `DungeonNode` (`0x30B13`): 19/50. 8 members author `SNAM = 0` and 9 are un-targeted. The 31 dropped include
    `dunBrokenFangCaveNode`, `dunDeadMensRespiteNode`, `MS13BleakFallsBarrowCampSceneNode` and `dunBluePalaceWingNode`.
  - Group `0x15FCE`: 5/21. Dropped include `WhiterunHoldScenes`, `ReachHoldScenes`, `RiftHoldScenes` and others.
  - `CompanionsRadiantNode` (random): 1/6. Dropped: `CompanionsAelaNode`, `CompanionsSkjorNode`,
    `CompanionsVilkasSplitNode`, `CompanionsFarkasSplitNode` and `CRReconKickerNode`.
  - `BQBranchNodeSHARES`: 4/9. Dropped: `BQPale`, `BQFalkreath`, `BQEastmarch`, `BQRift` and `BQReachNodeSHARES`.
  - `AssaultActorEvent`: 2/3 (`WIAssaultNode` dropped).
  - **Total: 58 members, 41 of them `SMQN`.**
- **Impact**: those quests can never be SM-started, and nothing logs it. A random parent is reduced to choosing among
  its reachable fraction (1 of 6 for Companions radiants). The loss is in the fail-closed direction, so it is coverage,
  not corruption.
- **Related**: ESM-2026-10-08-D4-01 (the direction). This finding holds under either reading: picking the `SNAM == 0`
  member as head instead leaves 7 of DungeonNode's 8 zero-segments detached.
- **Suggested Fix**: never drop a member. Rebuild each group's order so that every member is placed. If `SNAM` is the
  insertion-time predecessor, as xEdit's "Previous Node" suggests, insert each node after its referenced node and put
  `0`/dangling at the front, with deterministic tie-breaking. Warn when a group is not a single chain. Pin a real-data
  floor: every SM node reachable from its event root.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (event-root and branch-node child ordering in `build_story_manager_tree`)
- [ ] **TESTS**: A regression test pins this specific fix

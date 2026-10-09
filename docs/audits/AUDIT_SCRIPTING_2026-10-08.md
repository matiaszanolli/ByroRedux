# Scripting Subsystem Audit — 2026-10-08

**HEAD**: 00f580e09 · **Baseline**: `docs/audits/AUDIT_SCRIPTING_2026-10-05.md` (HEAD `a2c24b16e`) · **Audited**: Dim 1 (Recognizer Chain), Dim 2 (Fragment Dispatch & Locks), Dim 3 (Event Runtime), Dim 4 (Attach & Reference Identity), Dim 5 (Scene/Package/Dialogue/Cinematic + Story Manager) · **Unchanged since baseline (skimmed)**: Dim 6 (Legacy ObScript — no commits; guards spot-checked), Dim 7 (Provider/Extender — one `Expr::Is` scan arm; guards spot-checked)

This audit ran as part of `/audit-suite --preset comprehensive`. One agent analysed every dimension directly, with no
sub-agents. Each dimension went to `/tmp/audit/scripting/dim_N.md` before the next one started. The engine was not launched.
The evidence is static code reading, `cargo test`, and a read-only Python census of `Skyrim.esm` (SE). The census
walked the `SMBN`/`SMEN`/`SMQN` groups and their CTDAs. `Update.esm` authors no SM records, so the census matches the
`sm1-story-manager.sh` load order exactly: 571 nodes.

## What changed since 2026-10-05

There are 30 files and +4,779 / −133 lines in scope. The new surface is dominated by the **#5366 Story Manager**
(`crates/scripting/src/story_manager.rs`, 1,874 new lines, plus `systems/story_events.rs` and `sm.event`) and the
**#5367 dialogue layers** (greeting fallback, Random/Say-Once/Goodbye line lifetime, force-greet, the Skyrim
`ForceGreet` dialect, and per-line voice). Also in scope:

- `63bf3347f` (#3817): cinematic tether release and cell re-adoption.
- `9813af435` (#4415): the racial spell pair.
- `dbc07e8f0` (#5271): FO3/FNV INFO `QSTI` ownership.
- Fixes for every baseline finding except #5300: #5297, #5298, #5302, #5304, plus #5307, #5071 and #5074.

## Build & test state: CLEAN

```
$ TMPDIR=/mnt/data/tmp cargo test -p byroredux-scripting -j 8                     524 passed; 0 failed; 3 ignored
$ <1.96.0 cargo> test -p byroredux --bin byroredux -- boot::schedule cell_loader::references reference_enable_gate \
    npc_dialogue cinematic strip_ active_tether attach::tests extensions interaction story forcegreet quest
                                                                                   267 passed; 0 failed; 1 ignored
$ cargo test -p byroredux-scripting -- obscript papyrus_provider compatibility    78 passed; 0 failed; 1 ignored
```

None of the findings below is caught by a test. The Story Manager unit tests build synthetic trees, which have single
unbroken chains, catalogued condition functions, and quests that are never completed. The live gate `sm1` encodes the
current walk's own output as its expected set.

## Executive Summary

**Verified this cycle:**

- **#5297.** The spoken path returns direct advances without claiming the journal.
- **#5298.** `resolve_npc_actor` declines when the resolved actor is the player, in all three arms.
- **#5071 / #5074 / #5302 / #5307.** Lock inventory, docs and Access row are all correct.
- **#5304.** The kill-on-load recognizer is gated on exact vanilla EDIDs and master provenance.
- **`StoryEvent`.** It is a correct Pattern-B marker: the drain is unconditional at the head, and the `cleanup.rs`
  contract row and test are present.
- **Story Manager locking.** The walk is two-phase (a read-only walk, then starts under the write guard), and the
  alias-fill/bindings writes are deferred past the stages guard (`cbaf782db`).
- **`RunOn::EventData`.** It fails closed on L-tags and unknown tags.

**New findings**: 0 CRITICAL, 1 HIGH, 4 MEDIUM, 1 LOW (6 total). Already tracked: 1 (#5300, partly addressed).

The Story Manager is live: it starts quests on every CLOC/KILL/AHEL. Against real Skyrim data, four independent
defects decide which quests start:

- **HIGH (SCR-D5-2026-10-08-01).** Node conditions on uncatalogued CTDA functions evaluate as 0.0, and 98 of them then
  pass vacuously. Both quest starts that `sm1` gates on are vacuous passes of this kind.
- **MEDIUM (SCR-D5-2026-10-08-02).** A processed non-sharing node consumes the event even when it started nothing. The
  CK's own tutorial says consumption happens only when the node "actually manages to fire a quest".
- **MEDIUM (SCR-D5-2026-10-08-03).** The single-`SNAM`-chain walk silently drops 41 quest nodes that sit on detached
  chain segments. Among them are the Companions radiant givers, 5 bounty holds and 31 dungeon starters.
- **MEDIUM (SCR-D5-2026-10-08-04).** Phase B ignores `start_quest`'s refusal. A completed or failed radiant stays
  "eligible" forever, gets logged as started and spends its pool slot. A stopped quest also resumes without the
  start-reset that Skyrim applies.

**ESM-2026-10-08-D4-01 verdict (the `SNAM` direction): not independently decidable from the scripting side. It is
corroborated in part and not refuted.** See § ESM-D4-01 cross-check.

**Untrusted-input verdict (runtime-side decoders):**

- `SCDA`: no production change, so still **NO panic / OOB / unbounded recursion / OOM**.
- Provider manifests and provider lowering: unchanged, and the caps hold.
- The SM tree build is bounded. A cycle or a headless group cannot loop the walker: the visited bitmap is per event,
  arena indices are pre-resolved, and dangling pointers become `None`. A modded tree cannot panic it, because every
  index comes from `by_form_id`. The frontends' verdict is in `AUDIT_PAPYRUS_2026-10-08.md`.

## Decline-Invariant Audit (Dims 1, 5, 6)

| Decline point | Verdict |
|---|---|
| `classify_guard_atom` / `split_and`, `While`/`If` exceptions, `MAX_CONDITIONAL_DEPTH` | Holds; untouched |
| Player-receiver declines (`SetUnconscious` / `StartCombat` / `StopCombat`) | **Fixed** at apply time by #5298 (`fragment/effects.rs:169-185`) |
| `prim_add_race_spells` / `prim_remove_race_spells` | **Lowers a script-defined helper by name and discards the receiver.** SCR-D1-2026-10-08-01 (MEDIUM, latent) |
| `Expr::Is` (FO4 `is`, #5322) | `is_side_effect_free` and the compatibility scan both handle it. `expression_mentions_provider` has no arm and falls to `_ => false`. The result is a silent miss or a whole-handler decline, which is safe, so it is not filed |
| Named `CallArg` binds positionally | Existing: #5068, unchanged |
| **SM node CTDA evaluation** | **Leaks.** Unmodeled functions are evaluated as 0.0 and can pass, which then starts a quest. SCR-D5-2026-10-08-01 (HIGH) |
| SM `RunOn::EventData` L-tags / unknown tags | Holds: the condition fails |
| ObScript VM caps; unknown command → traced no-op | Holds; no change |

## Runtime Lifecycle Matrix (Dims 2–5)

| Invariant | Verdict |
|---|---|
| `StoryEvent` marker (Pattern B) | Holds. The drain at `story_manager.rs:437-441` comes before any early return. A one-slot overwrite is possible: an ECS observation, not filed |
| KILL / CLOC / AHEL producer timing | KILL and CLOC are raised in Update before the dispatcher, so they dispatch the same frame. AHEL is raised in Late (selection) or Update (force-greet) and dispatches next frame. None is lost |
| SM two-phase lock shape | Holds. `StoryManagerNodeState` is written under the `QuestStageState` write inside an exclusive system, with no reverse acquirer. The `raise_hello_story_event` nest is CONC-D3-2026-10-08-01 |
| SM consume rule | **Wrong trigger** (SCR-D5-2026-10-08-02) |
| SM child order coverage | **Drops detached segments** (SCR-D5-2026-10-08-03) |
| SM quest start | **Refusal ignored; no start-reset** (SCR-D5-2026-10-08-04) |
| Journal subscribers / cascade / populate-replace | Hold. The #5297 fix is verified |
| CTDA OR-precedence / empty list = true | Untouched |
| Cinematic retention lifetime | Release at the route terminal, plus re-adoption (#3817). The cell pick is ECS-D7-01. The release doc's claim about `vehicle` is SCR-D5-2026-10-08-05 (LOW) |
| Force-greet locomotion | The step is written into `GlobalTransform` and discarded by propagation. That is ECS-2026-10-08-D6-01, cross-referenced and not re-filed |

## Findings

### HIGH

#### SCR-D5-2026-10-08-01: Story Manager node conditions on uncatalogued CTDA functions evaluate as 0.0 and pass vacuously, so the dispatcher starts quests on unmodeled terms. Both of `sm1`'s gated boot starts are such passes

- **Severity**: HIGH (skill escalation: a quest-start effect fires on an **unmodeled** term instead of declining)
- **Dimension**: Scene/Package/Dialogue (Story Manager); evaluator side Dim 3
- **Untrusted-Input**: No (vanilla data triggers it)
- **Location**:
  - `crates/scripting/src/story_manager.rs:659-668`: `walk_siblings` evaluates `node.conditions` through the shared
    evaluator and starts on `true`.
  - `crates/scripting/src/condition.rs:191-229`: `from_index`. Anything outside the 22-entry catalog is `Unknown`.
  - `crates/scripting/src/condition.rs:995-1001`: `Unknown` → `0.0`.
- **Status**: NEW. It is the same policy class as GAME-D5-2026-10-08-01 (fail-open FNV Dialogue packages → force-greet),
  but that finding covers a different consumer. Here the consumer starts quests.
- **Description**: the M47.1 "unknown function → 0.0" default was designed for display and selection gating. The SM
  dispatcher reuses it to decide quest **starts**. Skyrim's SM vocabulary is mostly outside the catalog:
  `GetInCurrentLoc` 359, fn 576, 606, 565, `GetGlobalValue` 74, `GetQuestRunning` 56, 629, `GetRandomPercent` 77 and
  others. Any of these compared against a value that 0.0 satisfies (`== 0`, `< 1`, `!= 1`, `<= x`) passes no matter what
  the world state is.
- **Evidence** (census of Skyrim.esm SM node CTDAs):
  - Overall, 939 CTDAs: 759 uncatalogued (81%), 98 passing at 0.0.
  - Under the three events that have live producers (KILL/CLOC/AHEL): 328 / 256 / 62, and **40 quest nodes carry at least
    one vacuously-passing term**.
  - Seven quest nodes pass on nothing but such terms. Two of them are exactly the two starts `sm1-story-manager.sh`
    asserts:
    - `WIGreetingNodeSHARES` (`0x000C791B`): sole condition fn **145** `== 0`. The smoke's own comment says it is
      "passing trivially at boot".
    - `CWChangeLocationScenes` (`0x000D5176`): sole condition fn **56** = `GetQuestRunning(CWFinale 0x000D1444) == 0`.
  - `docs/engine/story-manager.md` §7 describes these as "genuinely condition-gated nodes".
- **Impact**:
  - Every CLOC/KILL/AHEL event can start quests whose authored gate was never evaluated. Some conditions also fail
    closed in the same way: the other 661 unknown terms block their nodes regardless of state, which silently loses
    coverage.
  - In the CLOC chain, `CWChangeLocationScenes` starts a Solitude/Windhelm map-table scene quest on the first location
    change anywhere (the boot cell is Whiterun Dragonsreach), and SCR-D5-02 then makes it consume the event.
  - The quest-state corruption is persistent, because SM-started quests are saved.
- **Related**: GAME-D5-2026-10-08-01, GAME-D5-2026-10-08-05 (the fn-0 mapping), ESM-2026-10-08-D4-02 (field decode),
  SCR-D5-2026-10-08-02.
- **Suggested Fix**: give the evaluator a tri-state result (`Known(bool)` / `Unmodeled`), and have SM dispatch treat
  any `Unmodeled` term as a decline. A node whose gate cannot be evaluated does not start or consume. Catalog the cheap,
  state-backed functions first: 56 (`QuestStageState`), 74 (`Globals`) and 77 (an RNG). Re-derive the `sm1` expected set
  afterwards, and pin a census test asserting that no live-event quest node starts through an `Unknown` term.

### MEDIUM

#### SCR-D5-2026-10-08-02: A processed non-sharing quest node consumes the event even when it starts nothing, which contradicts the CK tutorial's rule. Once its pool is running, `CWChangeLocationScenes` swallows every CLOC

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Dialogue (Story Manager)
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/story_manager.rs:664-675`: consumption keys on `evaluate(..)` passing plus `kind == Quest &&
    !shares_event`, and does not check whether a candidate was queued.
  - `:712-730`: `process_quest_node` returns with no candidate when nothing is eligible.
  - `:605-621` (the doc) and `docs/engine/story-manager.md` §7 Phase 3 ("keyed on processing … not on a quest actually
    starting").
- **Status**: NEW
- **Description**: the design reads "the event will be consumed … as soon as it finishes with that node" from the *SM
  Event Node* page as keyed on processing. Bethesda's own *Bethesda Tutorial Story Manager*
  (`/mnt/data/src/reference/ck-uesp-wiki/(main)/Bethesda Tutorial Story Manager.wiki:49`) states the trigger
  explicitly: "without this box, **if a node actually manages to fire a quest**, the event will not go through the rest
  of the tree". The same page's *Num quests to run* text also has the node "attempt" starts that can fail.
- **Evidence**: `CWChangeLocationScenes` is a stacked, non-sharing node. Its pool is `CW00SolitudeMapTableScene`,
  `CW01SolitudeMapTableScene`, `CW00WindhelmMapTableScene` and `CW01WindhelmMapTableScene` (census). Its gate passes
  almost always (SCR-D5-01). Each CLOC starts the next pool entry. Once all four are running, `eligible` is empty, so no
  candidate is queued, yet the walk still sets `consumed = true`. From then on, every node the walk visits after it gets
  no CLOC: `IntroScenesNode`, `CompanionsNode` (with `C00LocationMonitoringNode*` and `CR08`/`CR09LocationMonitoring`)
  and `QuestNode`. The reverse `SNAM` reading (ESM-D4-01) only changes which nodes are starved: the WI/Dungeon/BQ/Favors
  branches instead.
- **Impact**: location-driven quest progress below any always-passing non-sharing node stops permanently after its pool
  fills. Vanilla relies on such nodes failing to start (alias fill, running quest) without blocking the tree.
- **Related**: SCR-D5-2026-10-08-01, SCR-D5-2026-10-08-04. In vanilla, alias-fill failure is the usual reason a start
  "fails". The runtime has no alias-gated start (see Future-Phase Readiness).
- **Suggested Fix**: set `consumed` only when the node queued (and phase B actually started) at least one quest. Because
  phase B decides the start, move the consume decision into a per-event replay, or re-check `start_quest`'s result per
  candidate. Correct the doc's reading, and add a test where a non-sharing node whose quest is already running lets a
  sibling below it fire.

#### SCR-D5-2026-10-08-03: `build_story_manager_tree` follows a single `SNAM` chain per sibling group, so 41 Skyrim quest nodes on detached chain segments are never evaluated. Among them are the Companions radiant givers and five bounty holds

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

#### SCR-D5-2026-10-08-04: Phase B ignores `start_quest`'s refusal. Completed and failed radiants stay "eligible" forever, are logged as started, consume pool marks and rewrite event fills, and stopped quests resume without Skyrim's start-reset

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Dialogue (Story Manager); quest lifecycle touchpoint Dim 2
- **Untrusted-Input**: No
- **Location**:
  - `crates/scripting/src/story_manager.rs:486-524`: `stages.start_quest(..)`'s `Option` return is discarded. The
    fired/last-fire marks, the `started_event_fills` push and the "started quest" `info!` all follow unconditionally.
  - `:712-727`: eligibility is `!is_running`.
  - `crates/scripting/src/quest_stages.rs:172-195`: `start_quest` returns `None` unless the status is `Stopped`.
  - `crates/scripting/src/quest_stages.rs:240-254`: completed and failed quests also cannot be stopped.
- **Status**: NEW
- **Description**:
  - **Completed/Failed quests.** Phase 3 moved eligibility from `is_started` to `is_running` so radiants can re-fire.
    But a quest that reached a QSDT Complete/Fail stage is neither running nor startable. In a stacked pool it is
    `preferred[0]` on every event, so later entries never start. In a random or do-all pool it periodically wins the
    pick and wastes the event. Each time, phase B logs "started quest …", marks it fired, and overwrites its
    `StoryEventAliasFill`.
  - **Stopped quests.** These resume with their old `stages_done` and stage. The CK *Quest Data Tab*
    (`ck-uesp-wiki/(main)/Quest Data Tab.wiki`) says "Run Once: Prevents the Quest from being reset when it starts",
    which means a non-Run-Once quest **is** reset on start. A re-fired radiant therefore keeps its previous run's history,
    and `GetStageDone` gates plus non-repeatable `SetStage`s see a finished run.
- **Impact**:
  - Radiant pools jam once an entry completes.
  - The `sm1` attribution grep (`started quest … via node …`) can pass on a start that never happened.
  - Reruns of stopped radiants are semantically wrong.
- **Related**: SCR-D5-2026-10-08-02. The consume and start decisions should use the real start result.
- **Suggested Fix**: treat `start_quest == None` as "not started": no marks, no fill, no log, and no consume. For SM
  starts of non-Run-Once quests (QUST `DNAM` flag), reset the quest (`QuestStageState::reset` + start) so that completed
  and failed radiants can rerun. Pin both behaviours.

#### SCR-D1-2026-10-08-01: `AddRaceSpells`/`RemoveRaceSpells` lower a *script-defined* helper by name and discard any receiver, so `x.AddRaceSpells()` on any object lowers onto the player

- **Severity**: MEDIUM. This is latent: the skill escalation (emit on an unmodeled term) would make it HIGH, but the
  runtime is currently a no-op on the player, per CHAR-2026-10-08-D4-02.
- **Dimension**: Recognizer Chain
- **Untrusted-Input**: Yes (`.pex` from possibly-modded archives)
- **Location**: `crates/scripting/src/translate/effects.rs:1444-1495` (`lower_race_spells`,
  `prim_race_spells_arity_ok`, both `prim_*`).
- **Status**: NEW. CHAR-2026-10-08-D4-02 covers the runtime *target* (the player never receives `RaceSpells`). This
  finding covers the lowering contract.
- **Description**: neither name is an `actor.psc` native. They are functions defined on MQ101QuestScript. The primitive
  lowers any unqualified call by name, without checking that the calling script's own definition is the vanilla body. It
  also accepts any `MemberAccess` receiver ("The receiver is NOT distinguished") and always emits `ActorRef::Player`. A
  mod script that defines its own `AddRaceSpells()` with other semantics, or calls `SomeAlias.AddRaceSpells()` on
  another script type, is lowered as a re-apply/clear of the player's racial spells.
- **Impact**: none today. Once CHAR-D4-02 stamps the player's `RaceSpells`, a mod's same-named helper would add or remove
  the player's racial spells and their constant modifiers.
- **Related**: CHAR-2026-10-08-D4-02, #4415.
- **Suggested Fix**: accept only the unqualified spelling, and only when the calling script defines that function
  argless (the `ScriptSource` has the function table). Decline every member-call spelling.

### LOW

#### SCR-D5-2026-10-08-05: The #3817 release doc says the exit-cart path "never reads `vehicle`", but `Effect::ExitCart` reads it for the exit root-motion rotation

- **Severity**: LOW (doc rot plus an unpinned equivalence)
- **Dimension**: Scene/Package/Dialogue (cinematic)
- **Untrusted-Input**: No
- **Location**:
  - `byroredux/src/systems/cinematic.rs:466-472`: the doc.
  - `:545-550`: the release clears `vehicle`.
  - `crates/scripting/src/fragment/effects.rs:1538-1548`: `ExitCart` derives `exit_root_motion_rotation` from
    `state.vehicle` + `vehicle_local_rotation`, falling back to the actor's own rotation.
- **Status**: NEW
- **Description**: after a terminal release, `ExitCart` takes the fallback branch. The result is equivalent only if the
  attachment system left the rider's `Transform.rotation` equal to `vehicle.rotation * local_rotation` on the last
  tethered tick. That is plausible for a parked cart, but nothing pins it, and the doc asserts the opposite of what the
  code does.
- **Impact**: none observed. A later change to the attachment cadence would silently change the MQ101 exit heading.
- **Suggested Fix**: correct the doc, and add a lifecycle test asserting `exit_root_motion_rotation` is unchanged across
  a terminal release, or have the release snapshot the composed rotation.

## ESM-D4-01 cross-check (the `SNAM` direction)

**Verdict: not independently decidable from the runtime side. Not refuted, and partly corroborated.**

- **The `sm1` expected set is not evidence.** Its boot set (`WIGreetingNodeSHARES` + `CWChangeLocationScenes`,
  `CRHoldExpansion` stopped) is the output of the current walk, and both starts are vacuous passes (SCR-D5-01).
- **Bethesda's "sharing nodes above non-sharing" guidance does not discriminate.** Pairwise census over every chain
  segment: SHARES-above-consumer pairs number 95 in the decoded "next" reading against 87 reversed; consumer-above-SHARES
  pairs are 87 against 95. That is symmetric.
- **FormID age corroborates ESM.** In `DungeonNode`'s reached segment, 17 of 18 `SNAM` edges point at an **older**
  FormID. That matches ESM's age signal: under the CK's append-at-bottom default, a node points at the node created
  before it.
- **Structure corroborates ESM.** Groups with several `SNAM = 0` members and forks (DungeonNode: 8 zeros, 9 un-targeted)
  show that `SNAM` is not a maintained linked list in *either* reading. That fits an insertion-time "previous node" hint,
  as xEdit names it, better than a maintained "next" pointer, but it does not prove it.
- **The tutorial text is about consumption, not direction.** "if a node actually manages to fire a quest" supports
  SCR-D5-02 regardless of order.

## Cross-referenced (filed today by sibling audits, not re-filed)

| ID | Covers |
|---|---|
| ESM-2026-10-08-D4-01 / D2-01 / D4-02 | `SNAM` direction; `RNAM` ×24; undecoded `QNAM`/`XNAM`/`MNAM`/`HNAM` |
| ECS-2026-10-08-D6-01 | `forcegreet_system` (and `eat_sleep_system`) write the step into `GlobalTransform`, which propagation discards, so a force-greet NPC never walks |
| CONC-D5-2026-10-08-01 | `PhysicsWorld` lock-order inversion in `forcegreet_system` / `eat_sleep_system` |
| CONC-D3-2026-10-08-01 / CONC-D4-2026-10-08-01 | `raise_hello_story_event` reads `LoadedCellIndex` under the `StoryEvent` write; the Access row omits `StoryEvent` |
| ECS-2026-10-08-D7-01 / D7-03 | Re-adoption picks the cell from the local `Transform`; `StoryEventAliasFill` keeps stale EntityIds |
| GAME-D5-2026-10-08-01 / D5-05 | Fail-open FNV Dialogue packages → perpetual force-greet; the fn-0 `GetButtonPressed` premise |
| GAME-D7-01 / D7-02 / D4-01 / D2-01 | SM aliases lost across a fresh load; location cursor survives an in-process load; KILL only from combat deaths; `forcegreet_open` ignores `player_can_act` |
| UI-D6-2026-10-08-01 | Escape closes the dialogue page without `end_open_conversation`, so INFO OnEnd is skipped |
| CHAR-2026-10-08-D4-02 | The racial-spell pair targets a player who never gets `RaceSpells` (see SCR-D1-01 for the lowering half) |
| AUD-2026-10-08-D5-01 | Dialogue voice PCM held in `SoundCache` forever |

## Existing findings re-verified

| Issue | State at HEAD |
|---|---|
| #5297, #5298, #5302, #5304, #5307 (baseline batch) | **Fixed** and verified in code. Closed |
| #5300 (Say Once / spoken-fragment replay) | **Still OPEN, partly addressed.** #5367 Phase L honours FO3/FNV `DATA` Say-Once through a global `DialogueSpokenInfoForms`. Skyrim's `ENAM` response flags are still undecoded, since `skyrim_data` is carried raw. The same-INFO re-select OnEnd asymmetry is untouched |
| #5071 (lock inventory) | Fixed. `RaceSpells` is also listed |
| #5068 (named `CallArg`) | Still open, code unchanged |
| #5065 (live Enable/Disable consumer) | Still open, code unchanged |
| #3817 (cinematic retention) | Closed by `63bf3347f`. The release doc claim is SCR-D5-05; the cell pick is ECS-D7-01 |

## Future-Phase Readiness

- **The SM start contract is the gating item.** Vanilla radiant gating is mostly *alias-fill failure*: a quest whose
  non-optional alias cannot fill does not start, and so does not consume the event. The runtime starts first and fills
  aliases afterwards (`refresh_scene_actor_bindings`), with no refusal path. The fixes for SCR-D5-01, 02 and 04 converge
  on one primitive: an SM start attempt that can fail (unmodeled gate, running/completed quest, required alias unfilled)
  and whose real outcome drives consumption, marks and logs. This gap is not on the
  `m47-3-quest-alias-design.md` "Remaining subsystem boundary" list.
- **Catalog growth now has gameplay weight.** Every function added to `ConditionFunction` turns SM vacuous or blocked
  terms into real gates, so the `sm1` expected set must be re-derived on each addition.
- **ObScript phase 2 / alias follow-ups** are unchanged.

## Findings Count

| Severity | New | Already tracked |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 1 | 0 |
| MEDIUM | 4 | 1 (#5300) |
| LOW | 1 | 0 |
| **Total** | **6** | **1** |

By dimension: Dim 1 has 1 MEDIUM. Dim 5 has 1 HIGH, 3 MEDIUM and 1 LOW. Dims 2, 3, 4, 6 and 7 found nothing new.

---
*Generated by the `/audit-scripting` skill (single-agent run). Suggested next step:*
`/audit-publish docs/audits/AUDIT_SCRIPTING_2026-10-08.md` (domain label `scripting`). Also add:

- `quests` and `game:skyrim` for SCR-D5-01 through SCR-D5-04 (FO4/SF share the walker);
- `dialogue` for SCR-D5-05.

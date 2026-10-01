# P4 Fixture — MS01 "The Forsworn Conspiracy" (frozen 2026-09-20)

The authored-objective/dialogue fixture for the playable slice's P4 phase,
in the spirit of [`p2-combat-fixture.md`](p2-combat-fixture.md): freeze one
piece of shipping Skyrim SE content, derive every value from the installed
master, and let the fixture name its own first blockers rather than
discovering them mid-implementation.

## Frozen anchors

| Anchor | Value | Derived by |
|---|---|---|
| Quest | `MS01` "The Forsworn Conspiracy" — FormID `0x00018B4B` | installed `Skyrim.esm` |
| Fragment script | `QF_MS01_00018B4B` — 38 stage bindings, 17 lowered today | `dump_stage_fragment_effects` |
| Objective-display stage | **15** → `SetObjectiveDisplayed(objective 10)` "Go to the Shrine of Talos" | production lowering |
| Objective chain | 35 → display 35; 36/46/66 → conditional complete+display pairs; 70 → complete 50/55, display 70, `SetStage 90`, reward; 90 → Eltrys disable/enable + ambush quest | production lowering |
| Objectives authored | 14 (indices 10–70), journal text present on all | `quest.show 0x18B4B` |
| Aliases | 36 authored, 2 bound at rest in the reference cell | live `quest.aliases` |
| Smoke cell | `WhiterunBanneredMare` (P0/P1/P2/P3 reference interior) | fixture system |

Why MS01: it is the objective-densest vanilla quest whose stage fragments
lower through the production recognizer chain today, it exercises the three
objective verbs (`SetObjectiveDisplayed` / `Completed` / conditional
guards), and its stage 15 displays an objective through the exact path the
native objective HUD consumes — already gated live by
[`p3-hud.sh`](../smoke-tests/p3-hud.sh).

**Stage 10 is a deliberate negative control**: its fragment only pokes the
`MS01MiscObjectives` helper quest and displays nothing — the HUD gate
asserts the objective line is absent before stage 15.

## Live capabilities the fixture can build on (2026-09-20)

- `quest.start` / `quest.setstage` dispatch through the canonical
  fragment-effect path; `quest.show` reports stage + objective state.
- `QuestObjectiveState` + `QuestStageState` + `QuestDefinitionRegistry`
  drive the native objective HUD (P3) — verified in a live Vulkan run.
- DIAL/INFO records are parsed (`crates/plugin/src/esm/records/misc/dialogue.rs`:
  `DialRecord` with quest ownership, per-game category and `DLBR` branch; `InfoRecord` with responses,
  conditions, emotion) — the dialogue data is already in the index.
- The M47.1 condition evaluator (`GetStage`, `GetStageDone`, …) is the same
  one MS01's `StageDoneGuard` fragments and INFO CTDA branches need.

## First blockers (named by the fixture, in implementation order)

1. **NPC activation → topic selection.** `ActivateEvent` on an NPC must
   resolve that NPC's DIAL topics for running quests (MS01's are Eltrys-owned)
   and pick the first INFO whose CTDA list passes — the evaluator exists; the
   wiring and the "NPC owns topic" lookup do not.
   **Wired 2026-09-29:** the "NPC owns topic" edge is `DialRecord::quest_refs`
   plus the live alias bindings — `running_quests_binding_entity` (scripting)
   returns the running quests an entity is alias-bound to, and
   `systems::npc_dialogue` consumes the player's `ActivateEvent` on such an
   NPC, picks the first INFO through the SCEN path's own `select_first_info`
   (subject = actor, target = player), and stamps `NpcDialogueTopic` on the
   NPC (`dialogue.status` reads it). NPCs became interaction candidates
   (`InteractionKind::Npc`, "Talk" prompt) only when a running quest's alias
   actually binds them. Four bin tests drive the whole chain through the real
   alias fill + evaluator; one scripting test pins the ownership lookup.
   **Live route opened 2026-09-29 (same day):** the "no Eltrys entity" gap
   was a wrong-cell assumption, not a spawn defect — the authored data puts
   Eltrys in `MarkarthWarrens` (ref `0x000198FD`, persistent children,
   enabled; pinned by the `ms01_eltrys_authored_placement` probe). There the
   unique-actor fill binds him (`quest.aliases` reports alias 1 → entity),
   and the live route runs: start MS01 → activate → topic selected →
   response presents. The real content-side gap the probe surfaced was the
   parser's: Skyrim authors the quest→topic link as DIAL **QNAM**, which
   `parse_dial` did not read — every Skyrim DIAL had an empty `quest_refs`
   (117 MS01 topics were unowned). QNAM now feeds `quest_refs` beside QSTI.
   **Topic filtering and branch order (2026-09-30, #5037 / #5045):** owning a
   quest no longer means owning all 117 of its topics. The DIAL category is
   decoded per game (Skyrim+ `DATA` byte 1, not the flags byte 0), so MS01's
   5 Scene and 4 Miscellaneous topics drop out. A topic is listed only when an
   INFO passes for *this* NPC, which drops the other 13 speakers' prompts.
   `DLBR` branches (Skyrim's top-level group, FO4's under `QUST`) now order
   the conversation per the Creation Kit's model. A qualifying Blocking
   entry, such as `MS01EltrysBlockingShrineBranch01`, opens it, and its INFO's
   `TCLT` links become the list. Otherwise the Top-Level starting topics form
   the list. Mid-branch children such as `0x18A30` are reached only through
   links. The Hello greeting is still unmodeled: without a blocking entry,
   the first Top-Level topic stands in as the opening line.
2. **Response presentation.** A native dialogue surface (pause-menu-grade,
   like the inventory page) showing the INFO response text + topic list;
   no Scaleform dependency, per the P3 "native UI is the reference path" rule.
   **Landed 2026-09-29:** `GameMenuPage::Dialogue` in the native menu — the
   activation-driven selection opens it (serial-watermarked
   `DialogueSurfaceState`, so it opens once per fresh selection), the page
   shows the selected INFO's response text above the NPC's owned-topic list
   (captured into `NpcDialogueTopic.topics` at selection time), and a topic
   click emits `DialogueUiAction::SelectTopic`, lowered through
   `select_topic_by_form_id` — the same selection the activation path uses.
   The surface takes input focus but deliberately does not stop the
   simulation (vanilla dialogue is in-world and timed; this also keeps the
   debug-server drain alive, so a route smoke can assert on the presented
   response while the surface is up). Verified live in `MarkarthWarrens`:
   start MS01 → activate Eltrys (entity 930) → the surface opens with his
   authored line ("What? By the gods, Betrid....") resolved from the
   localized strings, the topic list showing the authored player prompts
   ("Is this your note? What does this mean?", "(Walk away)"), and the
   engine screenshot captures the presented surface. Two headless egui
   tests pin the response render + topic click and the shared-resume
   Close; the bin re-selection test pins the click's engine half.
3. **Objective-completion feedback.** The HUD objective line exists;
   completed→next-objective transitions (stage 36/46/66's pairs) need the
   same live consumer to prove visually.
   **Landed 2026-09-30 (#5152/#5153):** two consumer-facing gaps closed
   together. (a) *Dialogue fragments* — `parse_info` now decodes the INFO
   `VMAD` (xEdit `wbVMADFragmentedINFO`: 5 257 of 31 465 vanilla Skyrim
   INFOs bind OnBegin/OnEnd `TIF_` fragments), the M47.2 session walk
   lowers them into a `DialogueInfoFragments` table (2 714 from
   `Skyrim - Misc.bsa`), and the dialogue selection dispatches the spoken
   line's OnBegin binding (and OnEnd on selection-change / conversation
   close) through the same guard-free executor the quest/scene dispatchers
   use — a spoken line can advance its quest. (b) *Journal feedback* — the
   objective-state mutators emit bounded transition events, and the app
   frame composes them into the vanilla-style center-top announcements
   ("Objective completed — The Forsworn Conspiracy: Find evidence about
   Margret" / "New objective — …") through the real definition texts.
   Live-proven in the captured frame: the stage-36 conditional pair
   completes objective 20 and displays 22 with both announcements on
   screen alongside Eltrys's presented line.

**Known dialogue gap (deliberate, fixture-driven):** MS01's stage-20
blocking branch (`MS01EltrysBlockingShrineBranch01`, whose INFO fragments
set stages 13/82) is entered by Eltrys's FORCE-GREET, which stays
unmodeled — the player-activation path correctly selects the authored
top-level topic instead (`MS01EltrysNotAtShrineAttackTopicTopic` at
stages 10–20, whose own line carries no fragment). The blocking entry's
CTDA gates, decoded 2026-09-30 (`fn67` GetQuestRunning 0x16DF7, GetStage
MS01 ≥ 20, GetStage MS01 ≥ 12, GetIsID Eltrys base 0x13394), are pinned
here so the force-greet work starts from data. The dialogue-fragment
dispatch mechanism itself is bin-test-gated
(`the_spoken_lines_fragments_advance_the_stage` drives a synthetic TIF
world through the same selection path).

## Gates

1. One console-free route is the end goal; until the force-greet blocking
   branch lands, `quest.setstage` remains the objective-advance setup
   frontend (the same posture P2 held before E-key combat).
2. The route smoke is [`p4-quest-route.sh`](../smoke-tests/p4-quest-route.sh)
   (2026-09-30): loads MarkarthWarrens under TAA with the scripts +
   interface archives, gates the TIF populate telemetry, resolves Eltrys by
   expression, drives the activation → selection → presented response, and
   asserts the objective chain's live transitions (15 → display 10, 35 →
   display 35, 36 → complete 20 + display 22) plus the string-table
   resolution of the presented text. The dialogue-fragment stage advance
   is bin-test-gated until force-greet lands. It also caught, live, the
   empty-parent strings-discovery miss (a bare relative `--esm` launch
   silently lost all localization).
3. Add recognizers/condition functions only when this fixture trips on a
   missing one — the plan's no-speculative-breadth rule.

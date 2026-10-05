# #5350: OBL-2026-10-05-D2-01: The P4 topic-reachability model says every Oblivion topic is an opening list entry, but Oblivion's menu is the player's *known-topics* list: 1,761 of 3,184 Topic DIALs are reached only as INFO `TCLT` "Choices"

Labels: low,dialogue,gameplay,bug,game:oblivion,legacy-compat
Filed from: docs/audits/AUDIT_OBLIVION_2026-10-05.md

**Source**: `docs/audits/AUDIT_OBLIVION_2026-10-05.md` (OBL-2026-10-05-D2-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW. It is latent: Oblivion has no quest aliases, so `running_quests_binding_entity` (`crates/scripting/src/scene/quest_alias.rs:923`) returns nothing and no Oblivion NPC owns a topic today.
- **Dimension**: BSA v103 & ESM Data Slice. The routing owner is `/audit-gameplay` (the P4 dialogue mechanism). The Oblivion data half is owned here.
- **Location**:
  - `byroredux/src/systems/npc_dialogue.rs:27` (module doc: "Oblivion authors no branches, so its topics are all list entries").
  - `byroredux/src/systems/npc_dialogue.rs:135-138` (`TopicEntry::TopLevel` doc).
  - `byroredux/src/systems/npc_dialogue.rs:144-156` (`topic_entry`: a branch-less topic with `top_level() == None` → `TopLevel`).
- **Status**: NEW.
  - It is the Oblivion twin of #5224 (closed, FO3/FNV: "choice-only but would list as top-level"). #5224 fixed the Fallout branch only, through the FO3/FNV DIAL `DATA` flags byte, which Oblivion does not author.
  - `gh` searches for "AddTopic", "known topics", "Oblivion topic list" and "choice-only" find only #5224 and #3600 (PNAM ordering), both closed.
  - Today's reports do not cover the Oblivion half. GAME-D2-2026-10-05-01 is about FNV shared Top-level topics. The SCR note says the route is "unreachable by design" for Oblivion/FO3/FNV.
- **Description**:
  - #5037 and #5224 model the opening menu as "Top-Level entries; everything else via `TCLT` links". For Oblivion, branch-less and with no flags byte, every owned Topic is classified as `TopLevel`.
  - The Oblivion runtime does not work that way. Per the CS wiki:
    - `AddTopic.wiki`: "Only topics in this list [the player's known topics] can appear in an NPC's topic list". Topics enter that list through `AddTopic` or the INFO "Add Topics" box (`NAME`).
    - `Dialogue Tutorial.wiki`, "Decisions, Decisions": "The choices box will give the player a list of those topics only, after the line that lists them has been said".
  - So a Topic DIAL that appears only in some INFO's `TCLT` and is never added by `NAME` or a script is a choice-only follow-up, never an opening entry.
- **Evidence**: raw walk of `Oblivion.esm` this run (`/tmp/audit/oblivion/dial_census.py`):
  - DIAL `DATA` is 1 byte on all 3,817 DIALs. The categories are {Topic 3,184, Conversation 555, Persuasion 39, Combat 16, Service 14, Misc 5, Detection 4}.
  - Of the 3,184 Topic DIALs:
    - 1,777 are `TCLT` (Choices) targets.
    - 582 are `NAME` (Add Topics) targets.
    - **1,761 are `TCLT` targets and never `NAME` targets.**
    - 841 are neither; they are added by script `AddTopic`, or are GREETING-style.
- **Impact**:
  - Today: none, because no Oblivion NPC owns a topic.
  - Once Oblivion dialogue ownership is wired (an ObScript or condition-based owner in place of aliases), the opening menu would offer choice-only follow-ups. Examples are quest "Yes"/"No" answer topics and mid-conversation responses, which would be offered before the line that asks the question. That is the #5037 / #5224 failure mode, on Oblivion.
  - The doc states the wrong rule as fact, so the implementer of that wiring has no warning.
- **Related**: #5224 (closed, FO3/FNV twin), #5037 (closed), #3600 (closed), GAME-D2-2026-10-05-01, the SCR-2026-10-05 note on P4 reachability, and ESM-2026-10-05-D2-01 (INFO `DATA` decode; Oblivion row 3 B × 19,276).
- **Suggested Fix**:
  - Correct the two doc sites now. Oblivion's opening list is the player's known-topic set (`NAME` Add Topics + `AddTopic`), and `TCLT` targets are link-only.
  - When Oblivion ownership lands, give `topic_entry` an Oblivion arm that returns `LinkOnly` for topics never named by an INFO `NAME` (or by a known-topic set seeded from `NAME` / `AddTopic`).
  - Pin it with the 1,761 / 3,184 census.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it

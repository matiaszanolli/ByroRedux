# #5300 — SCR-D5-2026-10-05-01: Spoken-line fragments re-run on every selection, and the INFO "Say Once" flag is neither decoded nor honoured

- **Labels**: medium,scripting,dialogue,esm-plugin,game:skyrim,bug
- **Filed from**: `docs/audits/AUDIT_SCRIPTING_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5300

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Dialogue
- **Untrusted-Input**: No
- **Location**:
  - `byroredux/src/systems/npc_dialogue.rs:279-330`: `apply_selection` runs `speak_info_begin_fragment` unconditionally.
  - `byroredux/src/systems/npc_dialogue.rs:218-250`: `select_on_topic`'s fallback menu re-lists the spoken topic.
  - `crates/scripting/src/dialogue.rs:193-222`: `select_info` / `select_first_info` check only speaker and CTDA.
  - `crates/plugin/src/esm/records/misc/dialogue.rs:249-330`: `InfoRecord` has no ENAM field ("Skyrim+ INFOs author no
    `DATA`").
- **Status**: NEW. No issue mentions "say once". ENAM decode belongs to `/audit-esm`; the runtime gate belongs here.
- **Description**: before #5152, selecting a topic was presentation-only. It now executes the INFO's lowered OnBegin
  effects every time the INFO is selected: on every activation of the NPC, and on every UI click of a topic that is still
  listed. Topics that link nowhere fall back to the Top-Level menu, which includes the topic just spoken. Skyrim INFO
  `ENAM` bit 2 is "Say once" (UESP `Skyrim Mod/Mod File Format/INFO`). The CK's Topic Info page describes it as "this Info
  can only be said once by any given Actor. Once spoken, that actor will never say it again". There is no decode and no
  per-actor said-set, so the vanilla guard against replay is missing.

  There is also an asymmetry: re-selecting the **same** INFO runs OnBegin again but never the outgoing OnEnd. The filter
  is `existing.info_form_id != topic.info_form_id` (`npc_dialogue.rs:287-290`).
- **Impact**: a Say-Once reward or hand-off line whose conditions do not flip after it is spoken replays its effects on
  each click. Examples are `AddItem` to the player, `StartScene`, `Enable`, and objective sets. The result is item
  duplication and repeated scene starts. Lines whose `SetStage` flips their own conditions are self-limiting, so the
  blast radius depends on how the content is authored.
- **Related**: #5152, SCR-D2-2026-10-05-01, GAME-D2-01 (the FO3/FNV owning-quest gap).
- **Suggested Fix**: decode Skyrim `ENAM` flags onto `InfoRecord` (route to `/audit-esm`). Keep a saved `(actor, info)`
  said-set, written when a line is spoken. Have `select_info` skip a Say-Once INFO the actor has already said. Note the
  CK caveat that Say Once resets on reload for non-Start-Game-Enabled quests. Fire the outgoing OnEnd before re-speaking
  the same INFO.

Routing: the Skyrim INFO `ENAM` decode half belongs to the ESM parser (`esm-plugin`); the runtime said-set gate belongs to scripting.

_Source: `AUDIT_SCRIPTING_2026-10-05.md` (SCR-D5-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix

# #5380: SCR-D5-2026-10-08-01: Story Manager node conditions on uncatalogued CTDA functions evaluate as 0.0 and pass vacuously, so the dispatcher starts quests on unmodeled terms. Both of `sm1`'s gated boot starts are such passes

**Labels**: high,scripting,quests,bug,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5380

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-10-08.md` — `SCR-D5-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `walk_siblings` still gates on `evaluate(&node.conditions, …)` and `ConditionFunction::Unknown` still returns `0.0` (`condition.rs` ~995). Census numbers are the report's (Skyrim.esm), not re-run.

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other consumers of the `Unknown → 0.0` default that gate effects rather than display — e.g. FNV Dialogue package conditions (GAME-D5-2026-10-08-01))
- [ ] **TESTS**: A regression test pins this specific fix

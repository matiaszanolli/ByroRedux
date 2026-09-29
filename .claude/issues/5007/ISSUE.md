# PAR-D1-2026-09-29-02: MenuXml fetches an `<include>` before the cycle and budget checks, so fetch and inflate work grows quadratically with fragment size

**Labels**: medium,bug,import-pipeline,ui,safety

**Source report**: `docs/audits/AUDIT_PARSERS_2026-09-29.md`

- **Severity**: MEDIUM
- **Dimension**: Size Discipline
- **Location**: `crates/menuxml/src/parse.rs:676-719` (`splice_include`); production source `byroredux/src/hud.rs:205-207`
- **Status**: NEW (introduced by `20faaf89b`, the #4650 fix)
- **Trigger Input**: a prefab fragment `menus\prefabs\g.xml` whose body is N copies of `<include src="g.xml"/>`, meaning N self-includes. The same happens with any include after the 256-splice budget is spent.
- **Description**:
  - `splice_include` first runs `candidates.iter().find_map(|p| src.menu_xml(p))`. Only after that does it check `seen_includes` for a cycle and `splice_budget == 0`.
  - The production `HudAssets::menu_xml` is `self.misc.extract_or_warn(path)`: a full BSA extract plus inflate, with no cache.
  - Before #4650, the cycle key was the authored spelling and was checked before any fetch, so a self-include cost nothing.
  - Now every include element in every parsed fragment costs one fetch. That includes rejected cycles and everything after the budget is exhausted.
  - A fragment of S bytes therefore costs about S/22 fetches of itself, or O(S²) inflate. The budget allows up to 256 such fragments.
- **Evidence** (probe `menuxml-selfinc`, release build, each fetch inflating a zlib copy the way a BSA extract does):
  ```
  self-include fragment  22023 B ( 1000 includes): fetches  1001, inflated    22.0 MB,   9 ms
  self-include fragment 110023 B ( 5000 includes): fetches  5001, inflated   550.2 MB, 161 ms
  self-include fragment 440023 B (20000 includes): fetches 20001, inflated  8800.9 MB, 2.30 s
  ```
  - Four times the size costs about fourteen times the time. The compressed payload stays tiny because the text repeats, so the archive itself stays small.
  - Each cycle also logs one `warn!`, which gives 20,000 log lines for the last case.
- **Impact**: the `--hud` launch hangs on the main thread, with minutes of stall per multi-MB fragment. The trigger is a replaced or modified `Oblivion - Misc.bsa` / `Fallout - Misc.bsa`. Vanilla does not trigger it; the Oblivion, FO3 and FNV corpora pass.
- **Related**: #4650, PAR-D1-2026-09-21-04
- **Suggested Fix**:
  - Return early when `*splice_budget == 0` before touching the source.
  - Cycle-check every candidate spelling against `seen_includes` before fetching.
  - Cache fetched fragment bytes per `parse_document`, so a repeated include is a map hit.

**Validated at HEAD 9fcfdc3fc**: `splice_include` in `crates/menuxml/src/parse.rs` still runs `candidates.iter().find_map(|p| src.menu_xml(p))` before the `seen_includes` cycle check and the `*splice_budget == 0` check.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

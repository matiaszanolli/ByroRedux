# FNV-2026-10-18-03: Sunny Smiles's boot-time force-greet holds a modal dialogue page that clears injected input; four FNV live gates are red

- **Issue**: #6052 (https://github.com/matiaszanolli/ByroRedux/issues/6052)
- **Labels**: medium, gameplay, dialogue, test-gap, bug, game:fnv, legacy-compat
- **Source**: docs/audits/AUDIT_FNV_2026-10-18.md (snapshot as filed; GitHub is authoritative for state)

Found by the FNV compatibility audit of 2026-10-18 (`docs/audits/AUDIT_FNV_2026-10-18.md`, HEAD `af5063597`).

- **Origin**: D6-01. **Dimension**: smoke gates × dialogue.
- **Location**: `byroredux/src/app_events.rs:~725-734` (`release_world_input_for_ui` every frame while the dialogue page is open) → `byroredux/src/ui_input.rs:65` → `byroredux/src/interaction.rs:673` (`clear_debug_input`); opener `open_dialogue_menu` (`main.rs:~1433`).
- **Status**: NEW. No matching open issue; the 10-12 baseline ran no engine, so there is no earlier engine run to compare.
- **Description**: Booting `--game fnv --cell GSProspectorSaloonInterior --player`, Sunny Smiles (entity 157, PACK `00105CC7`) opens a `GREETING` conversation on the first frame, without activation. While that page is open the frame loop zeroes the gates' injected `input.hold` and `input.press`. The authored force-greet is probably correct behaviour (#5367); the fixtures have no way to dismiss it.
- **Evidence**:
  - `p1-character-traversal fnv` fails 3 times (twice on the desktop, once under xvfb): "interior walk-away was acknowledged but its countdown never started".
  - `p5-save-restart fnv` fails: "character did not move at least 10 units before saving".
  - `p5-door-transition fnv` fails: timed out waiting for exterior `wastelandnv (-17,0)`.
  - `p5-f5-f9-quicksave fnv` fails: "quickload did not restore the quicksaved pose".
  - `input.hold forward 3000` is acknowledged, then `player.status` shows `input_hold_frames_remaining=0` on the next command.
  - Passing: `p0-door-interaction fnv`, `m-exteriors fnv static`.
  - Dimension 1's own `p5-save-restart` attempts were inconclusive: the engine window was unfocused and desktop input leaked into it. Dimension 6's failure is consistent with the same cause but is not independently confirmed.
- **Attribution**: not bisected. README records "FNV P1 passes the complete Prospector round trip" on 2026-09-17. Candidates after that: the #5367 Phase F / #5376 ambient force-greet (10-07/08), `a95dc4ce3` (#5722), and the M47.3 work.
- **Impact**: these gates are the only live FNV coverage of walking, save/restart pose and door transition. Escalate to HIGH if a bisect shows the engine changed intended boot behaviour.
- **Suggested Fix**: a `dialogue.end` command called in the fixture prelude, or move the FNV fixture spawn outside Sunny's reach, or advance her quest stage. Check it against dt1's contract that quest-less force-greets must open.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the other FO3 / Oblivion / Skyrim paths that share this code
- [ ] **TESTS**: A regression test (or a repaired gate) pins this specific fix


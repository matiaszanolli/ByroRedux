# SCR-D5-03: no decompiled-.pex parity test for DA10

Filed as: matiaszanolli/ByroRedux#1740
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness
- **Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:325-358`
- **Labels**: low, legacy-compat, tech-debt, bug

## Description
The byte-equality fidelity gate (`recognizes_da10_and_reproduces_hand_builder`) runs `.psc` → AST → recognizer only. No test takes a DA10 `.pex`, runs `translate_pex`, and asserts the same hand-builder equality, so the decompiler→recognizer fidelity loop isn't closed by CI (corpus smoke is panic-only).

## Suggested Fix
Add a `translate_pex` parity test using a DA10 `.pex` fixture (or the hand-built writer).

# FNV-2026-10-18-06: `dt1-dialogue-layers.sh` fails at Phase G because Doc Mitchell's `onActivate` ObScript adds frames of latency

- **Issue**: #6055 (https://github.com/matiaszanolli/ByroRedux/issues/6055)
- **Labels**: low, dialogue, test-gap, bug, game:fnv, legacy-compat
- **Source**: docs/audits/AUDIT_FNV_2026-10-18.md (snapshot as filed; GitHub is authoritative for state)

Found by the FNV compatibility audit of 2026-10-18 (`docs/audits/AUDIT_FNV_2026-10-18.md`, HEAD `af5063597`).

- **Origin**: D5-02. **Location**: `docs/smoke-tests/dt1-dialogue-layers.sh:112-118`; cause `crates/scripting/src/obscript_objects.rs:709-751` (`deliver_activation`, since `6b9397ed2`). **Status**: NEW.
- **Description**: `GSDocMitchellScript` (`begin onActivate / if IsTalking == 0 / Activate / endif`) now consumes the `ActivateEvent` and re-queues a `NativeActivation`. That is 2-3 frames of latency, and the gate reads `dialogue.status` in the same debug drain with no retry.
- **Evidence**: shipped gate: `FAIL -- Phase G … "dialogue: no NPC carries a selected topic"`. With `dialogue.status` polled (40 × 0.5 s), Phases G, V and F all PASS; the game behaviour is fine and the gate is wrong. Because it exits at G, it also hides Phases V and F.
- **Suggested Fix**: poll `dialogue.status` for up to ~10 s after `script.activate`, as Phase F already does.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the other FO3 / Oblivion / Skyrim paths that share this code
- [ ] **TESTS**: A regression test (or a repaired gate) pins this specific fix


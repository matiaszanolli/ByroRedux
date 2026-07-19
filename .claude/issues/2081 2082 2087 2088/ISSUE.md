# Batch: FNV/FO3/Oblivion audit fixes

## #2081 FNV-D4-03 (byroredux-plugin) — dead real-data spot-check
tests.rs:432-441 keys Varmint Rifle on 0x000086A8 (never matches); real = 0x0007EA24.
Silent `if let` no-else. Fix key + hard assert. SIBLING: other silent if-let spot-checks.

## #2082 FNV-D6-01 (byroredux-core) — REAL BUG: text-key events on Reverse ping-pong
text_events.rs:20-46 visit_text_key_events infers wrap from curr<prev only; Reverse cycle
(stack.rs fold_reverse_time) trips wrap branch on backward legs → wrong keys fire.
Thread reverse_direction/cycle into fn; backward leg fires closed (curr, prev]. Add regression test.

## #2087 FO3-D6-01 (byroredux-nif) — doc: #1219 false "never hits it"
version.rs:429-466 NifVariant::detect one-shot warn comment claims vanilla never hits
(V20_0_0_4, uv=11). FALSE: meshes\triggers\collisionboxstatic.nif (vanilla FO3) hits it.
Impact zero (havok_scale_for maps Obl+FO3 both to 7.0). Update comment + regression note.

## #2088 DIM3-OBL-01 (byroredux-plugin) — doc: XESP mislabeled (Skyrim+)
walkers.rs:855-856 + mod.rs:517 label XESP "(Skyrim+)"; it's Oblivion-era (walkers.rs:788-791
+ tests/refr.rs:583-588 correct). Drop qualifier. SIBLING: grep cell/ for era-mislabels.

# FO3-SD-10: Stale statements in the FO3 skill and in race.rs

Labels: low,documentation,doc-rot,tech-debt,game:fo3,legacy-compat

**Severity**: LOW · **Dimension**: skill / doc rot · **Game**: FO3

## Description
Stale statements in the FO3 audit skill and in `race.rs`.

## Evidence
Location: `.claude/commands/audit-fo3/SKILL.md` (Game Context bullet "Known-open, do not re-file ... #4772" at line 29; Dim 5 scripting bullet "there is no general executing runtime for FO3 script logic" at line 104), `crates/plugin/src/esm/records/actor/race.rs:91-93`. Re-confirmed at HEAD.
1. #4772 is closed (`fc9622f6f`, 2026-10-19): `p2-melee-core.sh` now asserts `joints=17 live_joints=17`, `recoveries_total=0` and no explosive detach for FO3 (`P2_RAGDOLL_JOINTS=17` in `fo3.env`); the greps match `ragdoll_status.rs:97` and `commands/physics.rs:235-241`. `gh issue view 4772` reports CLOSED.
2. M47.3 object scripts attach and run on FO3 with no dialect gate (`references/attach.rs:352`, `obscript_objects.rs`); only the compiled-bytecode *compatibility analysis* has no FO3 dialect.
3. `race.rs:91-93` says the FO3 / FNV body textures live in the head-section table; the data has them under `NAM1` (see FO3-SD-04).

## Impact
Future FO3 audits skip a closed issue as "known-open" and under-report the FO3 scripting runtime.

## Related
FO3-SD-04, #4772.

## Suggested Fix
Update the skill's known-open list and Dim 5 scripting bullet, and the `race.rs` doc together with FO3-SD-04.

Source: docs/audits/AUDIT_FO3_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)

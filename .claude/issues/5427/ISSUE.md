# #5427: FNV-2026-10-08-D6-01: FNV's two new live gates break the smoke SKIP≠PASS contract and the index, and the M42 seat leg cannot tell which NPC sat

**Labels**: low,gameplay,ai,dialogue,bug,test-gap,game:fnv,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5427

**Source**: `docs/audits/AUDIT_FNV_2026-10-08.md` — `FNV-2026-10-08-D6-01` (HEAD `00f580e09`)

- **Severity**: LOW (test gap). It is the same class as #5276 (LOW).
- **Dimension**: Smoke Gates.
- **Location**:
  - `docs/smoke-tests/dt1-dialogue-layers.sh:1-70`.
  - `docs/smoke-tests/m42-eat-sleep.sh:1-102`: the launch at `:65-71` and the seat leg at `:101`.
  - `scripts/check-playable-smoke-contracts.sh:63` and `:98`: the only two gate lists it checks.
  - `docs/smoke-tests/README.md:20` (the contract) and `:56` (the index, which lists `dt1` but not `m42-eat-sleep`).
- **Status**: NEW. #5276 covers only `m48-5-fnv-hud.sh`. No 2026-10-08 report mentions either script's SKIP path.
- **Description**:
  - **No SKIP path.** Neither script has any `exit 77` / `SKIP` path. On a runner without FNV data, `--game fnv` boots and fails, and the script exits 1 ("engine exited waiting for …"). README:20 says missing data is "an explicit `SKIP` with exit code `77`, never a pass", and the #4724 sweep applied that to every other per-game gate.
  - **Not checked.** The contract checker loops over p0/p1/p2/p5/w1 and the five m48 HUD gates only.
  - **Not indexed.** `m42-eat-sleep.sh` is absent from README's run list and from CLAUDE.md's smoke list.
  - **Unscoped seat leg.** It waits for `'[m42] sleep npc='`. That log line (`byroredux/src/systems/eat_sleep.rs:215`) prints for any NPC that sits. After `time.set 23` every Sleep-package NPC in the saloon is a candidate, so the "seated the settler" PASS can come from a different actor.
  - The Skyrim gates added in the same window, `sm1-story-manager.sh` and `dt2-skyrim-forcegreet.sh`, share the first two gaps. They are named here for the fix, not counted.
- **Evidence**:
  - `grep -c 'exit 77\|SKIP'`: 0 in both FNV scripts.
  - `grep -c 'dt1\|m42-eat-sleep' scripts/check-playable-smoke-contracts.sh`: 0.
  - `wait_log "$LOG_DIR/session.stderr" '[m42] sleep npc='` does not interpolate `$entity`.
- **Impact**:
  - A dataless CI lane reports these gates red instead of skipped, which is the reason #4724 gave for keeping gates out of CI.
  - The M42 gate can pass with the target settler unseated.
- **Related**: #4724, #5276, GAME-D5-2026-10-08-02 (no FO3/FNV sleep marker is reachable, so the seat it gates is a sit-marker fallback).
- **Suggested Fix**:
  - Add the standard fixture-driven missing-data SKIP (exit 77 with a `smoke[<name>]: SKIP -- missing` banner) to both scripts, and add them, plus sm1/dt2, to the checker loop.
  - Index `m42-eat-sleep.sh` in README and CLAUDE.md.
  - Scope the seat leg to `npc=$entity`.

## Completeness Checks
- [ ] **SIBLING**: Same SKIP=77 path + checker-loop entry applied to the Skyrim gates added in the same window (`sm1-story-manager.sh`, `dt2-skyrim-forcegreet.sh`)
- [ ] **TESTS**: `scripts/check-playable-smoke-contracts.sh` covers the new gates (the contract checker is the regression pin)

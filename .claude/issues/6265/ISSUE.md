# #6265: SCR-2026-10-21-08: Challenge completion and reward-script order follow `HashMap` iteration

**Labels**: low,scripting,bug,game:fnv

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: LOW · **Dimension**: Legacy ObScript · **Location**: `crates/scripting/src/challenges.rs:227` (`note`: `for def in defs.0.values()`), `:278` (`challenge_system`), `ChallengeDefs(Arc<HashMap<u32, ChalRecord>>)` at `:86`

**Audit status**: NEW (same class as closed #6013)

**Untrusted-Input**: No

## Description

When one event or stat rise completes several challenges, the challenges complete, increment "Challenges Completed" and start their reward scripts in per-process hash order. "Challenges Completed" itself feeds misc-stat challenges, so which one finishes first can differ between runs.

## Suggested Fix

Iterate in FormID order: sort the finished list, or store the definitions in a `BTreeMap`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every `defs.0.values()` walk in challenges.rs (note, challenge_system, status/touched lists))
- [ ] **TESTS**: A regression test pins this specific fix

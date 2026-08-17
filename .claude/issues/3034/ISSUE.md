# ECS-2026-08-16-06: visit_text_key_events drops every text key when one frame advances a full clip period

**Issue**: #3034
**Severity**: LOW
**Dimension**: 10 — Animation Runtime
**Labels**: `low,animation,bug`
**Source report**: `docs/audits/AUDIT_ECS_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ECS_2026-08-16.md` (Dimension 10 — Animation Runtime).

**Location**: `crates/core/src/animation/text_events.rs`:29-68, with `crates/core/src/animation/player.rs`:97-105

## Description

`visit_text_key_events` silently drops **every** text key when one frame advances a full clip period — the window it scans wraps onto itself, and the wrapped case yields nothing rather than everything.

## Evidence

Re-verified 2026-08-17 against `text_events.rs`:29-68 and the time-advance in `player.rs`:97-105.

## Impact

Text-key events (footsteps, sound cues, script triggers authored on the animation timeline) are lost on any frame long enough to cover a whole clip cycle — a hitch, a load spike, or a very short looping clip at normal frame rates.

The failure is silent: no event fires, nothing logs, and the animation continues correctly. Short clips are the common case for exactly the kind of cue this carries.

## Suggested Fix

Handle the full-period-advance case explicitly: emit each key once (the semantically defensible reading for a single frame) rather than falling through to the empty result. Whatever the choice, make it explicit rather than an artefact of the window arithmetic.

## Related

- `collect_text_key_events` (the sibling collector in the same module)
- #3031 (ECS-2026-08-16-03 — the other animation-runtime finding this sweep)

## Completeness Checks
- [ ] **WRAP-CASE**: The full-period and multi-period advances are handled deliberately, not by fallthrough
- [ ] **ONCE-EACH**: A key fires once per period, not N times for an N-period advance
- [ ] **SIBLING**: `collect_text_key_events` checked for the same window arithmetic
- [ ] **TESTS**: A regression test advances by exactly one clip period and asserts keys fire

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3034 --json state` when live state is needed.*

# AUD-2026-08-16-D1-01: spatial sub-track position frozen at dispatch

**Issue**: #3086
**Severity**: MEDIUM
**Labels**: `medium,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_AUDIO_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_AUDIO_2026-08-16.md` (Dimension 1 — spatial sub-tracks).

**Location**: `crates/audio/src/lib.rs` (`AudioEmitter` docstring vs the dispatch path)

## Description

The entity-path spatial sub-track position is **frozen at dispatch**, while `AudioEmitter`'s docstring promises a per-frame update.

The docstring describes a spatial sub-track *"anchored at the entity's world position"*, and the module docs say `audio_system` *"updates listener position"* — but the emitter's own position is captured once when the sound is dispatched and never refreshed.

## Impact

A sound emitted from a moving entity stays at the position the entity occupied at dispatch. Footsteps, weapon fire and any emitter on an actor or vehicle detach from their source as it moves.

The listener half *is* updated per frame, which makes the failure directional and easy to misread as a listener-pose bug rather than an emitter one.

## Suggested Fix

Refresh the spatial sub-track's position from the entity's `GlobalTransform` each tick in `audio_system`, alongside the listener update — or, if per-frame emitter updates are deliberately deferred, correct the docstring so it stops promising them.

## Related

- #3087 (AUD-D6-01 — stale scheduler-wiring comments in the same subsystem)

## Completeness Checks
- [ ] **DOC-TRUTH**: The docstring matches behaviour whichever direction is chosen
- [ ] **SIBLING**: `OneShotSound` and the streaming-music path checked for the same freeze
- [ ] **PER-FRAME-COST**: The update is measured — it runs per emitter per tick
- [ ] **TESTS**: A regression test moves an emitter and asserts the sub-track follows

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3086 --json state` when live state is needed.*

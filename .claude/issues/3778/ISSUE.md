# #3778 — AUD-2026-08-30-D4-02: is_music_active()'s docstring promises "playing or fading out", but stop_music drops the handle, so it reports false for the whole fade tail

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, audio, doc-rot, documentation

---

**Audit**: `/audit-audio` — `docs/audits/AUDIT_AUDIO_2026-08-30.md` (Dimension 4 — Streaming Music Lifecycle), HEAD `64f64480`
**Finding ID**: `AUD-2026-08-30-D4-02`

- **Severity**: LOW
- **Status**: NEW

## Location

`crates/audio/src/lib.rs:626-637` (`is_music_active` + its doc), `:613-630` (`stop_music`)

## Description

`stop_music` issues `handle.stop(fade)` and then sets `self.music = None` precisely so a later `play_music` doesn't see a stale reference. But `is_music_active` is

```rust
self.music.as_ref().map(|h| !matches!(h.state(), PlaybackState::Stopped)).unwrap_or(false)
```

— with the slot cleared it returns `false` **immediately**, for the entire fade-out during which kira is still rendering the tail. On the one live path that is `REGN_AMBIENT_CROSSFADE_SECS` = 3.0 s (`asset_provider/audio.rs:133`).

The docstring immediately above says:

> *True when music is currently playing **or fading out**. Useful for menu-toggle / cell-load gameplay logic that wants to avoid stacking music calls.*

A caller that trusts that sentence as a "may I start a new track?" gate gets the opposite of the intended answer during the exact 3-second window the gate exists for.

## Evidence

Re-verified at HEAD — `stop_music` ends with:

```rust
handle.stop(fade);
// Drop the handle so a future play_music call doesn't see
// a stale reference. Kira keeps the sound alive internally
// until the fade completes.
self.music = None;
```

and `is_music_active` reads only `self.music`.

## Impact

**Documentation-only today**: `grep -rn "is_music_active" byroredux/ crates/` finds no non-test caller.

Filed so the first MUSC/cell-music consumer — which will have to arbitrate against `dispatch_region_ambient_music` for the single slot — does not build on a false contract.

## Recommendation

Either correct the doc to describe what the function actually reports ("true while a handle is installed and not yet `Stopped`; `stop_music` clears the slot immediately, so the fade tail reports false"), or keep a `stopping: bool` beside the slot.

Both are cheap; the choice belongs with whoever wires the slot arbitration, and either way it should land with a test — neither `stop_music` nor `is_music_active` is currently covered by a default (non-`#[ignore]`d) test.

## Related

- #3775 (`AUD-2026-08-30-D4-01` — the sibling continuation gap over the same single music slot)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — any other handle-slot accessor in `crates/audio/src/lib.rs` whose doc promises a state the cleared slot cannot report
- [ ] **TESTS**: A regression test pins this specific fix — `stop_music` followed by `is_music_active` must match whatever the corrected contract says

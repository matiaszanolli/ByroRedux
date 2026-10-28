# AUD-2026-10-27-D1-02: The public `AudioWorld::play_oneshot` has no rustdoc; its contract paragraph sits on crate-private `oneshot_slot_ready`

Labels: low, audio, documentation, doc-rot
Source: docs/audits/AUDIT_AUDIO_2026-10-27.md

**Report**: `docs/audits/AUDIT_AUDIO_2026-10-27.md` · **Finding**: AUD-2026-10-27-D1-02 · **Severity**: LOW (doc-rot) · **Dimension**: Spatial Dispatch & Listener
**Status**: NEW (dates from eac602644 / #5738; carried through 3a50b3d5b and b2e542eee)
**Location**: `crates/audio/src/world.rs:338-385`

## Description
The "Fire-and-forget one-shot dispatch ..." paragraph runs straight into "#5738 - the shared `play_oneshot*` prologue ..." as one doc block attached to `pub(crate) fn oneshot_slot_ready`. That paragraph documents the #853 drop-on-inactive behaviour and the 256-entry FIFO cap. As a result the engine's main producer API, `pub fn play_oneshot` (`world.rs:377`), renders with no documentation. `play_oneshot_following` links back to it ("as `Self::play_oneshot`") and `oneshots_requested` refers to "calls to `Self::play_oneshot`".

## Evidence
`world.rs:338-359` is a single `///` run ending above `oneshot_slot_ready`; `play_oneshot` at `world.rs:377` has no `///` (re-verified at HEAD).

## Impact
Documentation only. Producers reading rustdoc lose the contract that a headless world drops the play up front and that the queue sheds its oldest entry at 256.

## Related
#6160 (the `queue_oneshot` fold), #5738, #853.

## Suggested Fix
Move the first paragraph block back onto `play_oneshot` and keep only the #5738 prologue note on `oneshot_slot_ready`.

## Completeness Checks
- [ ] **SIBLING**: `play_oneshot_following` and sibling pub fns checked for the same doc misplacement


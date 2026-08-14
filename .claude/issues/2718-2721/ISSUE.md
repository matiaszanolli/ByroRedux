# #2718 #2719 #2720 #2721 — Scaleform UI bundle (audit closeout, 2026-08-14)

Four issues from the 2026-08-12 `ui-deep` audit suite, fixed together because
three of them land in `crates/ui`.

## #2718 — FO4 host-method catalog validated against 3 of 311 menus
`SAFEUI-04`. The injected AVM2 adapter installed exactly one forwarder per
catalog entry (138) onto `BGSCodeObj`. Any `BGSCodeObj.Foo(...)` outside the
catalog is a call on `undefined` (AVM2 Error #1006), which aborts the executing
frame handler — a menu that renders but stops responding. The only guard was an
`#[ignore]`d 3-movie test.

**Fix**: install the union of the catalog and the methods the movie's own
bytecode calls (the existing `referenced_host_methods` scan, promoted out of
`#[cfg(test)]`). Corpus sweep widened to all 311 menus and un-ignored.

**Measured**: 131 distinct uncataloged methods across the corpus — including
the entire main menu (`StartNewGame`, `ContinueGame`, `PopulateLoadList`,
`DeleteSave`, …). All 311 movies scan cleanly and still round-trip injection.

## #2719 — UI overlay allocates a fresh VkImage every frame
`CONC-D7-UI-03`. `SwfPlayer::tick` ended with unconditional `self.dirty = true`,
so `render`'s `if !self.dirty` early exit was dead code.

**Fix**: `dirty |= player.needs_render()` (the gate Ruffle's own desktop/web
hosts use), plus a content comparison of the readback so an unchanged picture
returns `None`. NOT done: recording the copy into `draw_frame`'s own command
buffer (suggested fix (b)) — that is a Vulkan sync change with failure modes
invisible to `cargo test`.

## #2720 — Navigator pump has two silent permanent-freeze paths
`CONC-D7-UI-04` / `SAFEUI-06`. (1) `NavigatorState::errors` was never cleared,
`first_error()` peeked at it, and `tick` returned early forever once it was
`Some`. (2) An unsettled preload returned from `tick` with no log, no error and
no state change.

**Fix**: `take_errors()` drains; failures are recorded (deduped, bounded) and
reported through `resource_errors()`; the stall wait is bounded, logged and
surfaced via `preload_stalled()`.

**Found while fixing**: the two paths are the same event. Ruffle sets
`awaiting_import` before an `ImportAssets` fetch and clears it *only* on
`load_asset_movie`'s success path, so a failed dependency wedges the preload
inside Ruffle — non-fatal error handling alone would have moved the freeze, not
removed it. The navigator now answers a failed fetch with a valid empty SWF so
the import completes with no symbols.

## #2721 — Three live "100-byte Vertex" doc sites
`TD3-2026-08-12-01`. Two were still live (`docs/engine/ui.md`,
`docs/engine/testing.md`); the third (`vulkan/pipeline.rs`) was already fixed by
`f6eb7fde`. Repo-wide sweep confirms the remaining `100 byte` hits are unrelated
record/block sizes.

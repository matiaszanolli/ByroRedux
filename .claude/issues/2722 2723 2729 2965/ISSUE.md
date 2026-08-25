# Issue Batch: 2722, 2723, 2729, 2965

## #2722 — SAFEUI-05 (byroredux-ui)
`crates/ui/src/player.rs:247-283` — `SwfPlayer::render`'s three failure paths
(`downcast_mut` → `None`, `capture_frame()` → `None`, `rgba.len() !=
pixel_buffer.len()` mismatch) all fall through to `self.dirty = false;
Some(&self.pixel_buffer)` — i.e. failure is reported identically to success,
and `dirty` being cleared means a real failure is never retried. Currently
LOW/unreachable (concrete backend type guarantees all three can't happen
today) but the code's stated intent (handle failure) doesn't match its
behavior. Suggested fix: return `None` and leave `dirty` set on any of the
three paths, so a genuine future failure retries instead of publishing a
stale/zeroed frame.

## #2723 — SAFEUI-07 (byroredux-ui + binary)
`byroredux/src/scene.rs:1139-1162`, `crates/ui/src/lib.rs:51-59,211-216` —
`UiManager::new`/`SwfPlayer::from_movie` fix the Ruffle viewport + offscreen
`TextureTarget` at the swapchain extent captured at setup time; nothing in
`main.rs` updates `ui_manager.width/height`, re-registers the UI texture, or
resizes Ruffle's target on swapchain recreate (window resize) — the overlay
just stretches (confirmed NOT a Vulkan hazard, `update_rgba` len assertion
can't fire). Separately `UiManager::close` is dead code (nothing calls it;
even if called, `App::ui_texture_handle` would still hold the registered
RGBA texture). Suggested fix: either wire a resize path (`set_viewport_dimensions`
+ re-register texture) or document the fixed-extent behavior; give `close()`
a caller or delete it.

## #2729 — TD3-2026-08-12-02 (docs)
`ROADMAP.md:628` still lists "input" as remaining M48 work, but input
routing shipped in `3ea5e275` (2026-07-27) — `crates/ui/src/input.rs`,
`byroredux/src/ui_input.rs`, focus transfer, modal capture, coordinate
scaling — and `docs/engine/ui.md` already documents it as shipped.
`docs/feature-matrix.md`'s UI table (6 rows) doesn't mention input/focus
routing at all, under-reporting the subsystem. Suggested fix: drop "input"
from ROADMAP.md's remaining-work list (keep font fidelity, menu lifecycle,
`_global.gfx`, Papyrus↔UI — genuinely open); add a
"Scaleform menu input routing + modal focus | ✓ M48" row to
feature-matrix.md's UI table. Trivial effort.

## #2965 — UI-D2-02 (byroredux-ui, MEDIUM)
`crates/ui/src/host.rs:415-431,458-464` — `normalize_call` treats the first
argument of ANY `SkyrimAvm1` host call as a `GameDelegate` request ID
whenever it's a finite non-negative integral Number, with no marker
distinguishing a real `GameDelegate.call` from a direct
`ExternalInterface.call`. `SkyrimAvm1` is the fallback profile for every
non-AS3 movie, so this over-fires on loose demo SWFs / third-party AVM1
content too — silently dropping the first real argument and attaching a
bogus `request_id`. Currently deferred-impact (no responses registered yet)
but structural: the first engine handler to land will get an argument list
short by one, or `respond` will re-enter with a bogus ID (`GameDelegate.as`
indexes `_callbacks[id]` — an AS-side error, invisible from either side).
Suggested fix: only strip when `catalog.find(method)` returns a `Request`
entry OR the movie has registered a `respond` callback
(`has_callback("respond")`) — both already available at the call site.
Always record the un-stripped argument list regardless of which branch
fires. Related: UI-D4-01 (same catalog-is-only-signal weakness on FO4 side,
NOT in this batch — note the interaction, don't fix it here).

## Domain classification
- #2722, #2965 → **ui** → `byroredux-ui`
- #2723 → **ui** (`byroredux-ui`) + **binary** (`byroredux` — scene.rs/main.rs)
- #2729 → **docs** — no crate test target, just ROADMAP.md/feature-matrix.md

## Plan
#2729 is a pure doc fix, do first (fast, zero risk). #2722 and #2965 are
single-crate (byroredux-ui) behavioral fixes. #2723 spans ui+binary and asks
for either a real resize-follow implementation OR documenting the fixed-extent
behavior — need to assess which is proportionate (a real Ruffle-resize +
texture re-registration wire-up is a bigger change than the other three;
lean toward the documentation route unless the resize path turns out simple)
before implementing.

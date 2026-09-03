# #3771 — UI-D1-2026-08-30-01: the live --menu route still inflates the movie three times and extracts the archive entry twice; prepare.rs's own doc claims two

**Severity**: LOW · **Location**: `byroredux/src/scene.rs`, `crates/ui/src/{player,lib,prepare}.rs`
**Source**: `docs/audits/AUDIT_UI_2026-08-30.md` (UI-D1-2026-08-30-01)

`prepare.rs` claimed a menu open costs "two inflates rather than four, and one tag walk rather
than two" — true of the crate, not of the only production caller. `scene.rs`'s `--menu` route
did its own `archive.extract(menu_path)` + `ScaleformProfile::detect(&root_bytes)` purely to
hand `load_swf_from_resource_provider` a `profile` argument that `prepare_movie` then used as a
mismatch-guard cross-check against its OWN detect on the *same bytes* — a second archive
decompression and whole-stream inflate to produce a value that could only ever tautologically
match. End-to-end cost: 2 archive extractions + 3 SWF inflates + 1 tag walk per menu open,
synchronously on the winit main-loop thread.

## Fix implemented

- `SwfPlayer::from_resource_provider`'s `profile` parameter widened to
  `Option<ScaleformProfile>`; passed straight through to `prepare_movie` (already `Option`).
  `Self::from_movie` now uses `prepared.profile` (the actually-detected value) instead of the
  caller-supplied one, correct whether `profile` was `Some` (guaranteed equal past the guard) or
  `None` (no caller value to fall back to at all).
- `UiManager::load_swf_from_resource_provider` widened to match, plus a new
  `UiManager::menu_profile() -> Option<ScaleformProfile>` accessor (mirroring the existing
  `host_bridge`/`host_object_state` pass-through pattern) so a caller passing `None` can still
  read the resolved profile afterward.
- `byroredux/src/scene.rs`'s `--menu` route: deleted the `archive.extract` + `ScaleformProfile::detect`
  pre-step entirely; calls `load_swf_from_resource_provider(..., None)` and reads
  `ui.menu_profile()` for the `ui.menu: loaded ... profile={:?}` line (kept, per
  `m48-menu-load.sh`'s grep contract) instead of pre-computing it. Net: one archive extraction
  (inside `from_resource_provider`), the crate's own "two inflates, one tag walk" — the archive
  route now matches what the crate always promised, instead of adding work outside the crate
  boundary.
- `prepare.rs`'s module doc extended (not renumbered — the crate-level cost claim was already
  accurate) to state explicitly that the number is end-to-end for the sole production caller,
  not merely crate-internal, so a future caller doesn't reintroduce the same redundant pre-step.

Regression test (issue's own TESTS checklist item, scoped to what's reachable without a Vulkan
harness — `scene.rs`'s route needs a real `VulkanContext`, out of unit-test reach):
`from_resource_provider_with_no_profile_still_resolves_it` proves the `None` path correctly
resolves the same profile `prepare_movie`'s own detect would give, using the same FO4 hudmenu
fixture the existing explicit-profile test uses — the correctness property the whole fix depends
on (the `None` path must not silently lose the profile scene.rs's log line needs).

**SIBLING** (issue's own checklist item): checked the loose-file `--swf` route
(`UiManager::load_swf` → `SwfPlayer::new`) — already auto-detects via `prepared.profile` with no
caller pre-extraction; no gap. `load_swf_with_profile` (genuinely explicit-profile callers) isn't
used from `scene.rs` at all. No other caller of `from_resource_provider` exists outside tests.

Full workspace: `cargo test --no-fail-fast` 7042 passing, 0 failing.

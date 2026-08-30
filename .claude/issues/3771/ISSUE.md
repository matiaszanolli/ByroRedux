# #3771 — UI-D1-2026-08-30-01: the live --menu route still inflates the movie three times and extracts the archive entry twice; prepare.rs's own doc claims two

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, ui, performance, bug

---

**Audit**: `/audit-ui` — `docs/audits/AUDIT_UI_2026-08-30.md` (Dimension 1 — Profile & VM Selection), HEAD `64f64480`
**Finding ID**: `UI-D1-2026-08-30-01`

- **Severity**: LOW
- **Status**: NEW
- **Profile**: both (archive route)

## Location

- `crates/ui/src/prepare.rs:16-17` — the cost claim
- `byroredux/src/scene.rs:1530-1539` — the only production caller
- `crates/ui/src/player.rs:210-214` — the second `provider.load(movie_path)`
- `byroredux/src/asset_provider/archive.rs:75-83` — `Archive::load` → `contains` + `extract`

## Description

`prepare.rs:16-17` states the post-#2968 cost as "**two inflates** rather than four, and one tag walk rather than two". That is true *of the crate*. It is not true of the only production caller.

`byroredux/src/scene.rs:1530-1539`:

```rust
Ok(archive) => match archive.extract(menu_path) {                  // archive extract #1
    Ok(root_bytes) => match ScaleformProfile::detect(&root_bytes) {  // whole-stream inflate #1
        Ok(profile) => {
            let (w, h) = ctx.swapchain_extent();
            let mut ui = UiManager::new(w, h);
            match ui.load_swf_from_resource_provider(Arc::new(archive), menu_path, menu_path, profile) {
```

`root_bytes` is used for **nothing but** `detect`. `SwfPlayer::from_resource_provider` then calls `provider.load(movie_path)`, which is `Archive::load` → `contains` + `extract` — a **second full decompression of the same archive entry** — hands the result to `prepare_movie`, which inflates it again (#2), before `SwfMovie::from_data` inflates a third time.

So the archive route costs **2 archive extractions + 3 SWF inflates + 1 tag walk** per menu open, synchronously on the winit main-loop thread, on FO4's multi-megabyte `hudmenu.swf` / `pipboymenu.swf`.

#2968 removed two of the five inflates the 08-27 audit counted; the two outside the crate boundary survived because the fix was scoped to `SwfPlayer`'s constructors — the commit body says so explicitly ("removing that would change the public loader signature and is left alone").

The `profile` argument `scene.rs` computes is not a cross-check either: it is the same function on the same bytes, so `prepare_movie`'s mismatch guard is tautologically satisfied on this route and can never fire.

## Evidence

Re-verified at HEAD: `prepare.rs` module doc still claims "two inflates … one tag walk"; `byroredux/src/scene.rs:1531` still calls `ScaleformProfile::detect(&root_bytes)` after its own `archive.extract`.

## Impact

Load-time only, so LOW. Filed because the module that exists to hold this property documents a number the shipping route does not meet — the exact drift class #2968 was opened for — and because the mismatch guard on this route is dead by construction.

## Related

- #2968 (CLOSED — the fix this is the residual of, outside the crate boundary)
- `docs/smoke-tests/m48-menu-load.sh` (greps the `ui.menu: loaded … profile={:?}` line, which must keep its value)

## Suggested Fix

`prepare_movie` already detects the profile unconditionally. Either:

- widen `UiManager::load_swf_from_resource_provider` to take `Option<ScaleformProfile>` and pass `None` from `scene.rs`, or
- return the detected profile from it so `scene.rs`'s `ui.menu: loaded … profile={:?}` line retains its value without the pre-extract.

Deleting `scene.rs`'s `extract` + `detect` pair removes one archive decompression and one whole-stream inflate. Then re-state the cost in `prepare.rs:16-17` as an end-to-end number, not a crate-local one.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the loose-file `--menu` route and any other caller that pre-extracts to detect
- [ ] **TESTS**: A regression test pins this specific fix — `SwfDecodeCounts` (added by #2968) should be assertable end-to-end from the scene-level entry, not only inside the crate

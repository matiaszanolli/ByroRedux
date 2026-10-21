# #6225: UI-D4-2026-10-21-01: Runtime `Loader` / `loadClip` movie fetches bypass the #5966 declared-length ceiling; Ruffle inflates them to the header's own `u32`

**Labels**: medium,ui,safety,bug

**Source**: docs/audits/AUDIT_UI_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Resource Navigator (also Profile, Prepare & Host Bridge, through the shim's documented contract) · **Location**: `crates/ui/src/navigator.rs:278-313` (`load_archive_resource`: `is_import_asset` gates both the ceiling and the shim; any other body is returned raw at `:284-286`). Ruffle `2edfd97d`: `core/src/loader.rs:1707` (`SwfMovieData::from_data` on a fetched `ContentType::Swf`) → …

- **Severity**: MEDIUM. This is a defence-in-depth gap on untrusted archive content, the same class and rating as #5966. It needs a crafted or corrupt replacement movie in an interface archive or loose override.
- **Dimension**: Resource Navigator (also Profile, Prepare & Host Bridge, through the shim's documented contract)
- **Profile**: both (`Fallout4Avm2` via `flash.display.Loader`; `SkyrimAvm1` via `MovieClipLoader.loadClip`)
- **Location**: `crates/ui/src/navigator.rs:278-313` (`load_archive_resource`: `is_import_asset` gates both the ceiling and the shim; any other body is returned raw at `:284-286`). Ruffle `2edfd97d`: `core/src/loader.rs:1707` (`SwfMovieData::from_data` on a fetched `ContentType::Swf`) → `core/common/src/tag_utils.rs:232` → `swf/src/read.rs:117-120` (`take(body_len)` with `body_len` from the header). Doc: `docs/engine/ui.md:882-894`.
- **Status**: NEW. It is a residual of closed #5966, whose fix and source guard cover only decode sites inside this crate. The shim half of the same gap (dependency movies reach Ruffle un-normalised) is open as #5480. No open or closed issue covers the ceiling on the Loader path.
- **Description**: `b93ccdf7f` put `ensure_declared_len_within_ceiling` in front of every `decompress_swf(` call in `crates/ui/src`, and its guard checks that every `SwfMovieData::from_data` in `player.rs` is fed by `prepare_movie`. Movies a running menu asks for are a different path. Ruffle's `Loader.load` / `loadClip` / `loadMovie` call `navigator.fetch`, and `load_archive_resource` runs `prepare_import_asset_swf` / `import_asset_paths` (both ceiling-gated) only when the archive path is already in `state.import_asset_paths`. That set is filled from `ImportAssets` tags only. Any other SWF body is handed back unchanged in `MemoryResponse`. Ruffle's loader then sniffs `ContentType::Swf` and calls `SwfMovieData::from_data(data, ..)`, which runs `swf::read::decompress_swf`. As measured in #5966, that reader inflates up to the header's claim (up to ~4 GiB), with `MAX_DATA_CAPACITY` bounding only the pre-allocation. None of the crate's guards can see a decode Ruffle initiates.
- **Evidence**: tag walk of the extracted vanilla movies.
  - `ImportAssets` targets are only `fonts_en.swf` (FO4 `hudmenu`, `pipboymenu`, `mainmenu`), and `gfxfontlib.swf` plus `Inventory components/*.swf` (Skyrim).
  - The same FO4 movies also name `Components/ConditionClips/Condition_Head.swf`, `Quests/DefaultBoy.swf` (HUD), `Pipboy_DataPage.swf`, `Pipboy_InvPage.swf`, `Pipboy_MapPage.swf`, `Pipboy_RadioPage.swf`, `Pipboy_StatsPage.swf`, `PipboyMobileSettings.swf` (Pip-Boy) and `Main_ModManager.swf`, `Main_LoginHelper.swf` (main menu). The `flash.display.Loader` and `URLRequest` symbols are present in each of these movies.
  - Skyrim `startmenu.swf` carries `MovieClipLoader` / `loadClip` and `BethesdaNetLogin.swf`.
  - None of these targets appears in an `ImportAssets` record, so every one of them reaches Ruffle raw.
  - The size of a Ruffle-side inflate was not measured here. The `u32::MAX`-over-1-MB-zlib result (1,073,741,819 B in 0.62 s, 1.1 GB RSS) is #5966's measurement, and it uses the same `swf` rev.
- **Impact**: a mod that replaces a Pip-Boy page, the HUD condition clip or `BethesdaNetLogin.swf` with a bomb header makes opening that menu allocate gigabytes. This can abort the process, and it happens on the main thread inside the navigator pump. The 128 MiB ceiling looks like a whole-path bound but is not one. The `ui.md` claim that the shim is "the one choke point every shipped SWF passes through before Ruffle sees it" is false for these fetches. The same fetches also skip the dialect shim, which is #5480's half and matters for Starfield's flag-clear `PlaceObject3` components.
- **Related**: #5966 (closed; this is its residual), #5719, #5480 (open: un-normalised dependency movies), `/audit-parsers`, `/audit-safety`.
- **Suggested Fix**: in `load_archive_resource`, run `ensure_declared_len_within_ceiling` on every body that `is_swf` (or that Ruffle would sniff as SWF), not only on import assets, and answer `record_degraded` on `Err`. Fixing #5480 at the same point, by running the shim on every SWF body, closes both halves. Add a navigator test that fetches a non-import URL serving `hostile_cws_header_bomb()` and expects a recorded degraded resource. Reword `ui.md:882` to name the navigator fetch as the second choke point.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_UI_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._

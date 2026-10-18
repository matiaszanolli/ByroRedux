# UI-D1-2026-10-17-01: The #5719 SWF ceiling only gates the shim's own inflate; every other decode path still honours the header's u32 (regression of #5719)

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/5966 (snapshot as filed 2026-10-17; GitHub is authoritative for current state)

**Source**: `docs/audits/AUDIT_UI_2026-10-17.md` (UI-D1-2026-10-17-01) · **Severity**: MEDIUM · **Dimension**: Profile, Prepare & Host Bridge Transport (also Resource Navigator, dependency path) · **Profile**: both · **Status**: Regression of #5719 (closed by `d601b45ab` as LOW; the fix is incomplete). Related open #5480.

## Description
The #5719 ceiling (`MAX_SWF_BODY_BYTES`, 128 MiB, `crates/ui/src/prepare.rs:120`) returns `None` from the dialect shim when a CWS header claims more than the ceiling, and its doc says `None` hands the movie to the stock loader. The stock loader has no such cap: `swf::decompress_swf` (ruffle `2edfd97d`, `swf/src/read.rs:72`) does `decompress_stream.take(body_len as u64).read_to_end(..)` with `body_len` taken from the header's own `u32`; `MAX_DATA_CAPACITY` only limits the pre-allocation.

So a CWS header claiming `u32::MAX` over a small zlib bomb is "declined" by the shim and then inflated to the claimed size on the very next line, and Ruffle's `SwfMovieData::from_data` decodes it again. Other sites that call raw `swf::decompress_swf` and never see the ceiling:
- `crates/ui/src/prepare.rs:290` (`prepare_movie`, right after the shim declines)
- `crates/ui/src/navigator.rs` `prepare_import_asset_swf` (~`:585-590`; runs before the shim on the dependency path) and `import_asset_paths` (~`:530-532`, runs again after the shim declines)
- `crates/ui/src/profile.rs:25` (`ScaleformProfile::detect`)
- `crates/ui/src/avm2_host.rs:79`, `:372`
- `crates/ui/src/avm1_host.rs:122` (`referenced_host_methods`)

ZWS (LZMA) has the same exposure: it is handed to the stock reader. The tests pin the shim in isolation (`cws_header_claiming_u32_max_over_a_bomb_is_declined` calls `decompress_zlib_after_header` directly), so they pass while the whole load path stays unbounded.

## Evidence
Measured with a scratch crate against the same pinned `swf` rev, outside the repo: a CWS header with length `u32::MAX` over a 1,042,919 B zlib body (1 GiB of zeros) goes through plain `swf::decompress_swf` as `Ok`, `data.len()` = 1,073,741,819 B, in 0.62 s, peak RSS 1.1 GB. Scaling to the header's 4 GiB claim is arithmetic, not measured. Orchestrator re-read `prepare_movie` and confirmed the fall-through and the raw call sites by grep.

Corpus context (header scan): `Skyrim - Interface.bsa` 53 swf (51 CWS, 2 FWS), largest declared 17,139,629 B (`fonts_cn.swf`); `Fallout4 - Interface.ba2` 311 swf, all CWS, largest 8,369,291 B (`fonts_ja.swf`). 128 MiB is ~7.8x the largest Skyrim movie. The documented FO76 `fonts_zhhant.swf` (65,902,267 B) was not verified (FO76 not installed).

## Impact
A crafted movie in a user-installed or mod interface archive can still force a multi-GiB allocation (potentially a process abort) when a menu or HUD opens. The 128 MiB constant looks like a bound but bounds only the shim's own, redundant buffer. Needs a malicious archive, hence MEDIUM rather than HIGH.

## Suggested fix
Add one shared pre-check, "container header declared length <= `MAX_SWF_BODY_BYTES`", for CWS, ZWS and FWS. Call it at the top of `prepare_movie`, `prepare_import_asset_swf`, `import_asset_paths`, `ScaleformProfile::detect`, the `avm2_host.rs` sites and `referenced_host_methods`, and before any `SwfMovieData::from_data`. Make an over-ceiling claim a hard `Err` (a recorded degraded dependency on the import path) rather than a `None` fall-through. Add a `prepare_movie`-level test with the bomb and `u32::MAX`.

Related: #5719 (closed, incomplete), #5480 (open: dependency movies reach Ruffle un-normalised), #5456, `/audit-parsers`, `/audit-safety`.

## Completeness Checks
- [ ] **SIBLING**: Every `swf::decompress_swf` / `SwfMovieData::from_data` site in `crates/ui` and `byroredux/` is routed through the pre-check (grep both)
- [ ] **SIBLING**: The Starfield dependency movies (`normalize_scaleform_dialect` path) and the navigator dependency path are covered
- [ ] **TESTS**: A `prepare_movie`-level regression test pins the bomb + `u32::MAX` header as a hard `Err`, not just the shim in isolation

# #5479: UI-D1-2026-10-08-01: the #4470 dialect shim decodes every menu movie a third time, ahead of `prepare_movie`'s single decode, for every profile; `SwfDecodeCounts` still reports one (regression of #2968)

**Labels**: low,ui,performance,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5479

**Source**: `docs/audits/AUDIT_UI_2026-10-08.md` — `UI-D1-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: #2968 is CLOSED; the extra inflate is present at HEAD (`prepare.rs` calls `normalize_scaleform_dialect(swf_data)` on raw bytes before `swf::decompress_swf`; `decode_counts` still a literal `decompresses: 1`), so filed as a regression.

- **Severity**: LOW. A redundant whole-stream inflate on a load path (menu open), not per frame.
- **Dimension**: Profile & Bridge (also Resource Navigator)
- **Profile**: both, plus Starfield
- **Location**:
  - `crates/ui/src/prepare.rs:54-83` (`normalize_scaleform_dialect`)
  - `crates/ui/src/prepare.rs:204-209` (the call ahead of `swf::decompress_swf`)
  - `crates/ui/src/prepare.rs:13-17` (module doc) and `:222-226`, `:250-253` (the `decode_counts` literals)
  - `crates/ui/src/navigator.rs:527-533`
- **Status**: Regression of #2968 (closed: "the archive load path decompresses the same SWF four times…").
- **Description**:
  - For a conforming CWS movie, which covers every Skyrim and FO4 menu, the shim inflates the whole stream
    (`decompress_zlib_after_header`, `read_to_end`) and walks its tags. It finds nothing and returns `None`.
  - `prepare_movie` then inflates the original bytes again, and Ruffle's `SwfMovie::from_data` inflates them a third
    time.
  - For an FWS movie, the shim instead makes a whole-file `to_vec()` copy.
  - The skill's invariant ("floor is two inflates + one tag walk"; "a stage taking raw bytes again is the regression")
    is broken, and the module doc's "two inflates rather than four" is now three.
  - `SwfDecodeCounts { decompresses: 1, … }` is a hard-coded literal. It does not count the shim, so the three #2968
    pins stay green.
  - Each nested `ImportAssets` dependency scan pays the same extra inflate (`navigator.rs:530`), on top of
    `prepare_import_asset_swf`'s own decompress.
- **Evidence**:
  - `prepare.rs:206-209`:
    ```
    let normalized = normalize_scaleform_dialect(swf_data);
    let swf_data: &[u8] = normalized.as_deref().unwrap_or(swf_data);
    let decompressed = swf::decompress_swf(swf_data)…
    ```
  - On the `None` path, `swf_data` is still the original CWS bytes.
- **Impact**: one extra full inflate of a movie the module doc calls "multi-megabyte" (FO4 `hudmenu.swf` /
  `pipboymenu.swf`), synchronously on the winit thread, per menu open and per imported dependency. No functional
  effect.
- **Related**: #2968 (closed), #4470, SAFE-D2-2026-10-08-01 (the same function's recursion)
- **Suggested Fix**:
  - Run the shim on `decompressed` (the `SwfBuf` `prepare_movie` already holds) instead of on the raw bytes.
  - Re-serialise to FWS only when a record was patched, and use those bytes for `PreparedMovie::data`.
  - Count the shim's decode in `SwfDecodeCounts`, so the pin measures the cost rather than asserting a literal.

## Completeness Checks
- [ ] **SIBLING**: The navigator import scan (`navigator.rs` `import_asset_paths`) gets the same single-decode treatment
- [ ] **TESTS**: A regression test pins this specific fix

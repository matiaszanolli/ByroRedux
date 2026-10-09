# #5480: UI-D4-2026-10-08-01: the shim normalises only the navigator's import scan; Ruffle receives dependency movies un-normalised, contrary to the shim's documented contract

**Labels**: low,ui,bug,game:starfield
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5480

**Source**: `docs/audits/AUDIT_UI_2026-10-08.md` — `UI-D4-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW. No visible effect today.
- **Dimension**: Resource Navigator
- **Profile**: Starfield (and any future dependency movie in the same dialect)
- **Location**:
  - `crates/ui/src/navigator.rs:252-313` (`load_archive_resource`)
  - `crates/ui/src/navigator.rs:524-536` (`import_asset_paths`)
  - `crates/ui/src/prepare.rs:35-53` (doc: "normalize … before Ruffle sees it")
- **Status**: NEW
- **Description**:
  - `import_asset_paths` normalises a private copy only to run its `swf::parse_swf` scan.
  - `load_archive_resource` then returns `body`, which is either the raw archive bytes or `prepare_import_asset_swf`'s
    raw-tag rewrite (never shimmed), as the `MemoryResponse` Ruffle parses.
  - The 9813af435 commit message ("dependency movies go through the same shim in the navigator") and the skill's Dim 1
    line both overstate this.
  - It is inert today:
    - The only fatal consumer of the bad records was this crate's own `swf::parse_swf`.
    - Ruffle's runtime `tag_utils::decode_tags` logs `Error running definition tag` and continues per tag (pinned Ruffle
      `0dde981`, `core/src/tag_utils.rs:75-78`).
    - So a dependency's class-name PlaceObject3 is skipped, while the root's is turned into an inert Modify.
- **Impact**:
  - The root and its dependencies take two different code paths for the same records.
  - The commit's live verification ("zero parse failures") counted only `log` lines. Ruffle's `tracing` errors for
    dependency records would not appear there.
  - A future upstream fix that models class-name placement through the shim would cover the root only.
- **Related**: #4470, UI-D1-2026-10-08-01
- **Suggested Fix**: apply `normalize_scaleform_dialect` to `body` in `load_archive_resource`, once, and reuse the
  result for the import scan. This also removes UI-D1-01's extra scan inflate. Alternatively, correct the docs to say
  that only the scan is normalised.

## Completeness Checks
- [ ] **SIBLING**: Root and dependency movies take the same normalisation path
- [ ] **TESTS**: A regression test pins this specific fix

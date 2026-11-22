# #7132: PAR-D2-2026-11-20-08: sfmaterial stream errors name no chunk index or class

**Labels**: low,import-pipeline,nifal,bug,game:starfield
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7132

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D2-08 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: crates/sfmaterial/src/reader.rs:1158 (`Cursor::read_bytes` -> `Error::UnexpectedEof { offset }` is chunk-relative), `ObjectTrailingBytes { leftover }`; call sites crates/sfmaterial/src/index.rs:259-266 (`build` loop)
- **Status**: NEW
- **Description**: `build` walks up to 1.44 M instances and propagates `consume_top_level_value`/`skip_top_level_value` errors with `?`. `UnexpectedEof` reports an offset relative to the chunk, and `ObjectTrailingBytes` a leftover count; neither names the stream position (`pos` is known in the loop) or class (`class_name` is known). A failure deep in the instance stream is not attributable.
- **Trigger Input**: any malformed instance chunk in a large CDB.
- **Suggested Fix**: wrap in `build`'s loop with `pos` and `class_name` (an `Error::AtInstance { index, class, source }`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix

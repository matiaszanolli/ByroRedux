# #4015 — REN-2026-09-06-D18-03: WTHR `DATA` bytes 1–2 are the only bytes #3883's naming pass left unnamed, and the one in-tree claim about them cites a decoder that never reads them

**Labels**: low, renderer, terrain-exterior, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D18-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `crates/plugin/src/esm/records/weather.rs` (`parse_weather_data`, the `WTHR_*_OFFSET` block, `SKYRIM_DATA_SIZE`); claim site `byroredux/src/scene/cloud_tile_scale_tests.rs` (module doc)
- **Status**: NEW
- **Description**: `e6d2811e` (#3883) replaced `parse_weather_data`'s bare
  literals with ten named `WTHR_<FIELD>_OFFSET` constants covering offsets
  3, 4, 5, 6, 8, 10, 11, 12, 15, 17, 18, plus byte 0 via `data.first()`. Bytes 1
  and 2 are skipped entirely — neither named, nor read, nor documented as
  reserved, in either the constant block or the function's doc comment. The
  offset block's own stated purpose (*"the byte layout … is stated once instead
  of scattered across twenty ungreppable integers"*) leaves a two-byte hole with
  no statement at all.

  The tree does contain a claim about them, but it is unverifiable in place:
  `cloud_tile_scale_tests.rs`'s module doc asserts *"DATA bytes 1-2 are
  cloud_speed_lower / cloud_speed_upper, NOT scales — see `weather.rs` DATA
  arm"*. The DATA arm it points to has never decoded those bytes, and
  `WeatherRecord` has no `cloud_speed_*` field (the old `cloud_speeds: [u8; 4]`
  was a *DNAM* mis-decode removed by #535 — `parse_wthr_dnam_is_texture_path_not_speeds`
  pins that it is gone). So the citation resolves to nothing either way.
- **Evidence**:
  - `parse_weather_data` goes from `data.first()` (offset 0) straight to
    `WTHR_TRANSITION_DELTA_OFFSET = 3`.
  - `grep -rn "cloud_speed" --include="*.rs"` finds exactly one live hit outside
    tests/issue archives: the `cloud_tile_scale_tests.rs` module doc.
  - Per-layer cloud motion for FO3/FNV comes from `ONAM` and for Skyrim from
    `RNAM`/`QNAM` (`WeatherRecord::cloud_layer_velocities`), so if bytes 1–2 do
    hold cloud speeds they are a second, unread source; if they do not, the
    comment is wrong. Either way the tree currently answers "unknown".
- **Impact**: Low today — no consumer depends on those bytes. But it is a
  two-byte gap in the one place the record's layout is now supposed to be
  stated authoritatively, propped up by a cross-reference that cannot be
  followed. Given that the same function's doc records one prior layout
  correction (*"byte 10 is thunder/lightning frequency and byte 11 is the
  classification bitmask (not byte 14)"*), an unstated hole is the shape a
  future correction gets wrong.
- **Related**: #3883 (`e6d2811e`), #535 (the DNAM `cloud_speeds` mis-decode),
  #529 (`cloud_tile_scale_for_dds`), REN-2026-09-06-D18-01.
- **Suggested Fix**: Resolve the two bytes against xEdit's shared WTHR `DATA`
  definition and either name them (`WTHR_UNKNOWN_1_OFFSET` / the real field
  names) with a one-line doc, or state explicitly in the offset block that
  bytes 1–2 are unread and why. Then correct or delete the
  `cloud_tile_scale_tests.rs` claim so it stops citing a decoder that does not
  contain the answer.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

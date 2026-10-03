# #5181: EXT-D4-2026-10-02-04: Stale pointers in sky/weather docs after #5087 and #4925, plus a misplaced rustdoc block in `weather.rs`

**Labels**: low,terrain-exterior,documentation,doc-rot
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW (doc rot)
- **Dimension**: Sky, weather, sun
- **Location**:
  - `docs/engine/skyal.md:555`: names `context/draw.rs` for `interior_portal_sky_preserves_room_weather_gate`. It is now `crates/renderer/src/vulkan/context/frame_params.rs:1201-1202`; the doc landed in e4df3abb9 at 09:14 on 2026-10-01, and c57e5cc4a moved the test the same day.
  - `crates/renderer/shaders/include/clouds.glsl:317`: "The host packs `[dir.x, speed, dir.z, 0]` (`build_composite_params`)". The packer is `pack_sky_dome`, at `frame_params.rs:1065-1070` since #4925.
  - `crates/renderer/shaders/composite.frag:575`: `draw.rs::hdr_clear`. It lives in `context/begin_frame_recording.rs:125`. This is older than this pass.
  - `byroredux/src/systems/weather.rs:83-119`: the rustdoc blocks for `pick_tod_pair` (`:83-91`) and `compute_sun_arc` (`:92-110`) sit above `SUN_SOUTH_TILT` (`:111-120`). Rustdoc attaches all three to the const, and `compute_sun_arc` (`:122`) and `pick_tod_pair` (`:161`) render undocumented. This has been the case since 2026-06-02.
- **Status**: NEW
- **Tier Violated**: n/a
- **Game Affected**: n/a
- **Description / Impact**:
  - Readers following skyal.md's gate table, or the GLSL wind-packing comment, land in a file that no longer holds the code.
  - The wind-packing comment matters most: `wind_consumers_match_the_host_packing` exists because this exact swizzle was misread once already.
  - The two TOD functions behind the sun arc and palette have no rendered docs.
- **Suggested Fix**:
  - Repoint the three references: `frame_params.rs`; `pack_sky_dome`; `begin_frame_recording.rs`.
  - In `weather.rs`, move the "Walk a `build_tod_keys` table…" block to directly above `pick_tod_pair`, and the "Derive sun direction…" block to directly above `compute_sun_arc`.

---

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Every other copy of the stale text (specs, code comments, skill files) updated in the same change
- [ ] **TESTS**: If a source-scan or doc-sync test can pin the corrected statement, add it

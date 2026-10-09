# #5411: AUD-2026-10-08-D1-01: Neither new crate surface has a default-lane guard. The #3086 follow site is outside the unit-seam scan, and `with_start_delay` is untested

**Labels**: low,audio,bug,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5411

**Source**: `docs/audits/AUDIT_AUDIO_2026-10-08.md` — `AUD-2026-10-08-D1-01` (HEAD `00f580e09`)

- **Severity**: LOW (test gap; the code is correct by trace)
- **Dimension**: Spatial Dispatch & Unit Seam
- **Location**:
  - `crates/audio/src/lib.rs:978-981`: `let position = bu_to_audio_space(gt.translation); … active.track.set_position(position, …)`.
  - `crates/audio/src/lib.rs:1513-1519`: `with_start_delay`.
  - `crates/audio/src/tests.rs:1433-1459`: the unit-seam guard.
- **Status**: NEW
- **Description**:
  - **Unit seam.** `every_kira_position_site_goes_through_the_unit_seam` pins the two listener sites and the two `add_spatial_sub_track` sites. `b4f08089b` added a third kind of kira position site, the per-tick emitter follow. Reverting its `bu_to_audio_space` to raw `gt.translation` would leave the guard green and silently place every moving emitter 70× too far away (inaudible past about 43 cm, which is the failure mode #3178 guarded). The only test of the follow, `emitter_position_follows_the_source_entity_regression_3086`, is `#[ignore]`d (device) and asserts the counter, not the units.
  - **`with_start_delay`.** It is the new crate API behind multi-segment voice and has no test of any kind. Device-free assertions are possible for:
    - the `StartTime::Delayed` setting;
    - shared `frames` (`Arc::ptr_eq`);
    - negative-delay clamping.
- **Evidence**: `grep -n "with_start_delay\|sync_emitter_positions" crates/audio/src/tests.rs` finds only doc and test-struct initialisers. No assertion covers either.
- **Impact**: A regression in either site would be silent in CI.
- **Related**: #3086 (closed), #3178.
- **Suggested Fix**:
  - Add the needle `"let position = bu_to_audio_space(gt.translation)"` (whitespace-squeezed) to the unit-seam test.
  - Add a headless unit test for `with_start_delay`'s settings and shared frames.

## Completeness Checks
- [ ] **SIBLING**: Every kira `set_position` / spatial site enumerated in the unit-seam guard
- [ ] **TESTS**: A regression test pins this specific fix

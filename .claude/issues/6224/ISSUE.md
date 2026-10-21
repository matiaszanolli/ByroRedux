# #6224: TD9-2026-10-21-01: four closed-HIGH fixes are guarded only by `#[ignore]`d real-data tests that are missing from `.github/real-data-pins.txt`

**Labels**: medium,esm-plugin,gameplay,test-gap,tech-debt,bug

**Source**: docs/audits/AUDIT_TECH_DEBT_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 9 · **Location**: `.github/real-data-pins.txt`; tests at `crates/plugin/src/esm/reader.rs:1691`, `byroredux/src/asset_provider/tests/fo4_mswp_corpus.rs:19`, `byroredux/src/npc_spawn/tests.rs:921`, `:3426`

- **Severity**: MEDIUM (skill severity row: an `#[ignore]`d closed-HIGH guard absent from the pin list)
- **Dimension**: 9
- **Location**:
  - `crates/plugin/src/esm/reader.rs:1691` `installed_starfield_small_masters_do_not_collide` (#6118, HIGH, `0a2955d33`, lane `plugin-lib`)
  - `byroredux/src/asset_provider/tests/fo4_mswp_corpus.rs:19` `fo4_mswp_canvas_swap_matches_the_picture_frame_nif` (#5755, HIGH, `f81e6e722`, lane `bin`)
  - `byroredux/src/npc_spawn/tests.rs:921` `installed_default_idle_clips_exist_in_the_meshes_archive` (#6100, HIGH, `abd3d5001`, lane `bin`)
  - `byroredux/src/npc_spawn/tests.rs:3426` `installed_fallout_spawner_shells_resolve_a_model` (#6075, HIGH, `011ad0c07`, lane `bin`)
- **Status**: NEW (the 10-19 drift, #6183, was repaired on 10-20, and these four HIGH fixes landed the same and next day)
- **Effort**: trivial
- **Description**: The pin file's contract ("New closed-HIGH pins append here") was not followed by any of the window's four closed-HIGH fixes with real-data tests. Each fix also has hosted synthetic tests (for example `starfield_two_small_masters_do_not_share_a_global_id`, `mswp_bare_bnam_matches_every_nif_spelling`, `oblivion_and_fallout_idle_paths_are_not_shared`, `a_model_less_spawner_shell_takes_the_leveled_picks_model`), so the logic is pinned. The real-data leg that proved each fix against vanilla data is now runnable only by name. Three of the four skip silently when data is missing (`eprintln!` + `return`, Existing #6184 pattern); `installed_default_idle_clips…` calls `require_game_data`. The other 16 new ignored tests guard MEDIUM/LOW fixes or features and are correctly unpinned.
- **Evidence**: `grep -c '<fn>' .github/real-data-pins.txt` returns 0 for all four; issue labels were checked with `gh issue view`.
- **Impact**: `scripts/run-real-data-pins.sh` (the only real-data regression run, since no runner is registered, #6182) does not exercise these HIGH fixes. A data-dependent regression of #6118 (Starfield small-master FormID collisions) or #6075/#6100 (Fallout spawn models / Oblivion idles) would go unnoticed.
- **Related**: #6183 (closed), #6182, #6184, FNV-2026-10-21-03 (`run-real-data-pins.sh` miscount).
- **Suggested Fix**: Append the four rows. Optionally make `every_real_data_pin_names_an_ignored_test_in_its_lane` also fail when a test whose doc comment cites a `#NNNN` labelled `high` is ignored but unpinned. That needs a checked-in label list, since a hosted test cannot query GitHub; simpler is a `/fix-issue` checklist line.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._

# #5439: NIF-D3-2026-10-08-02: #5232 hardened two of the three archive openers. `open_mesh_archive` and the Oblivion drift corpus still treat "present but unopenable" as "absent"

**Labels**: low,nif-parser,nif,bug,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5439

**Source**: `docs/audits/AUDIT_NIF_2026-10-08.md` — `NIF-D3-2026-10-08-02` (HEAD `00f580e09`)

- **Severity**: LOW (test-gap)
- **Dimension**: Block Dispatch Coverage (real-data harness)
- **Game Affected**: all, through the single-archive callers. Oblivion through the drift corpus.
- **Location**:
  - `crates/nif/tests/common/mod.rs:380-401` (`open_mesh_archive`: `Err(e) => { eprintln!("skipping: failed to open …"); None }`);
  - `crates/nif/tests/oblivion_stream_drift_corpus.rs:64-73` (private data-dir resolution), `:94-97` (`Err(_) => continue`).
- **Status**: NEW. #5232 (closed) named only `open_all_mesh_archives` and `open_ba2_by_name`. This is the sibling sweep it did not reach, and the third pass of the #4660 class.
- **Description**:
  - `open_mesh_archive` has 6 callers, and every one does `let Some(archive) = … else { continue/return }`. They are:
    - `real_archive_torch_meshes_surface_particle_emitters`, the #4467 emitter gate the skill cites;
    - `parse_rate_smoke_all_games`;
    - `vanilla_archives_have_zero_nisequencestreamhelper`;
    - `translation_completeness` ×2;
    - `ragdoll_import`;
    - `mtidle_motion_diagnostic`.

    If the BSA/BA2 reader rejects a vanilla archive, each of these skips that game green.
  - `oblivion_stream_drift_corpus` makes the same mistake with its own opener. If `Oblivion - Meshes.bsa` fails to open while any DLC opens, the drift detector runs green over the DLC alone. Its data-dir resolution also bypasses `game_data_dir`, which has two consequences:
    - it ignores the `BYROREDUX_REQUIRE_GAME_DATA` strict lane (#3850);
    - an explicitly-set `BYROREDUX_OBLIVION_DATA` that names a non-directory skips, instead of panicking as #3850's binding-override rule requires.
  - `open_optional_mesh_archives` only `eprintln!`s on an open failure. That is documented as deliberate for the account-varying tier, and I do not flag it.
- **Impact**: a reader regression of the #5008 shape would let these gates pass while measuring nothing. The highest-value one is the Oblivion no-`block_sizes` drift detector.
- **Related**: #5232, #4660, #5008, #3850.
- **Suggested Fix**: give `open_mesh_archive` the same panic-on-present-but-unopenable arm. Route `oblivion_stream_drift_corpus` through `game_data_dir`, and panic when an archive is present but cannot be opened, at least for the base `Oblivion - Meshes.bsa`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all six `open_mesh_archive` callers and every other private archive opener under `crates/nif/tests/`)
- [ ] **TESTS**: A regression test pins this specific fix

# #7409 SK-D5-01: The real-data `ArchiveSet` agreement test exercises 3 Skyrim SE archives, not the series the index exists for

**Labels**: low,import-pipeline,test-gap,bug,game:skyrim,legacy-compat

**Source**: `docs/audits/AUDIT_SKYRIM_2026-11-23.md` — finding `SK-D5-01` (HEAD `e01ca71f0`, re-validated against `origin/main` `4e68ab7b7` at publish).

- **Severity**: LOW
- **Dimension**: Archives + corpus gates (test-gap)
- **Location**: `byroredux/src/asset_provider/tests/archive_set.rs` (`the_index_agrees_with_the_scan_on_real_archives`, the `"Skyrim SE"` pool list: `Skyrim - Meshes0.bsa`, `Skyrim - Meshes1.bsa`, `Skyrim - Textures0.bsa`, `Dawnguard.bsa`, `Update.bsa`)
- **Status**: NEW (no open or closed issue matches; `gh issue list --state all --search "ArchiveSet index"` is empty)
- **Description**: `Dawnguard.bsa` and `Update.bsa` are Skyrim LE archives. Skyrim SE folds those DLCs into the main archives and ships neither (the SE `Data` has 23 BSAs, none by those names), so the filter `Archive::open(..).ok()` drops them silently and the SE leg of the test checks 3 archives (its own output: `Skyrim SE: 3 archives, 118728 lookups`). The commit message's "agrees on every entry of real FNV, Skyrim SE and Fallout 4 archives" therefore covers neither `Textures1`..`Textures8` (the series the sibling auto-load feeds), nor the five CC / Anniversary archives, nor `Animations`/`Misc`/`Sounds`/`Voices` (the other pools). The test cannot detect an ordering regression across a real multi-archive Skyrim pool (the thing the index changed).
- **Evidence**: test output above; `ls Data/*.bsa` on the SE install shows no `Dawnguard.bsa` / `Update.bsa`. The gap is closed for this audit by the probe: the same agreement property over the full 23-archive SE set in profile order (864,805 lookups) and the 9-archive LE set (663,775) has 0 disagreements and 0 hash collisions, so there is no live defect.
- **Impact**: none today; the index is correct on real data. A future change to push order, sibling expansion or key hashing would not be caught by the Skyrim leg.
- **Related**: ae0a768f7 (LMC-PERF-003), #6670.
- **Suggested Fix**: Name the SE series in the list (`Skyrim - Textures0..8.bsa`, `Skyrim - Animations.bsa`, a CC archive) and open the pool through `open_with_numeric_siblings` so the test also covers the real sibling order; keep the LE names under an LE leg through `skyrim_le_data_dir`.

## Completeness Checks
- [ ] **SIBLING**: the FNV and FO4 legs list archives that exist on those installs
- [ ] **TESTS**: the SE leg opens the real series through `open_with_numeric_siblings`

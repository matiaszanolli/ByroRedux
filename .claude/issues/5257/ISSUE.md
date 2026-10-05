# #5257: FNV-2026-10-05-D6-02: No FNV gate or bench loads Update.bsa, which the --game fnv profile lists last in every pool

Labels: low,test-gap,import-pipeline,bug,game:fnv,legacy-compat
Filed from: docs/audits/AUDIT_FNV_2026-10-05.md

**Source**: `docs/audits/AUDIT_FNV_2026-10-05.md` (FNV-2026-10-05-D6-02) · **Severity**: LOW · **Dimension**: Real-Data Validation · **Status**: NEW

## Description
- `assets/debug_profiles.toml` (#3790 / #3896 / #3916) lists `Update.bsa` **last** in every FNV pool (`default_bsas`, `default_textures_bsas`, `default_sounds_bsas`) — patch-archive precedence is treated as an FNV invariant.
- Every live FNV harness passes explicit archive lists **without** `Update.bsa`: the bench-of-record `prospector` args, the smoke fixture, and `m-exteriors.sh fnv`.
- Only the unit test `later_listed_bsa_wins_a_mesh_path_collision` exercises patch-archive precedence.

## Location
- `scripts/fsr-bench-matrix.sh` — the `prospector)` case `ARGS`
- `docs/smoke-tests/fixtures/fnv.env` — `FIXTURE_ARCHIVE_ARGS`
- `docs/smoke-tests/m-exteriors.sh` — `fnv_run`
- Compare: `assets/debug_profiles.toml` — `default_bsas = ["Fallout - Meshes.bsa", "Update.bsa"]`

## Evidence
`Update.bsa` holds 55 NIFs: 19 `architecture\mccarran`, 12 `nvdlc42\clutter`, 6 `landscape\lod`, 6 `architecture\novac`, 5 `architecture\ncr`, 3 `architecture\strip`, one each under hooverdam / nvhooverdam / freeside / nvdlc42 architecture. None lies on the Prospector, Goodsprings (-17,0) or Lake Mead (20,13) routes, so today's gates lose no content.

## Impact
- A regression in patch-archive precedence (the #3896 class) passes every live FNV gate.
- A future gate on McCarran, Novac or the Strip would silently render pre-patch meshes.

## Related
#3790, #3896, #3346.

## Suggested Fix
Add `--bsa "Update.bsa"` (and the textures/sounds equivalents) **last** to the FNV fixture, `m-exteriors.sh fnv` and the bench `prospector` args — or switch them to `--game fnv`. Land the bench-harness change byte-identically across the next same-machine control so the bench-of-record is not perturbed.

## Completeness Checks
- [ ] **SIBLING**: Other games' fixtures / bench args checked against their `debug_profiles.toml` pools for omitted patch archives
- [ ] **TESTS**: At least one live FNV gate exercises a path that `Update.bsa` overrides (or asserts the archive is mounted)

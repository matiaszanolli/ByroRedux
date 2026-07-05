**Severity**: LOW · **Dimension**: Block Dispatch Coverage · **Source**: `docs/audits/AUDIT_NIF_2026-07-05.md` (NIF-D3-001 + NIF-D3-002 + NIF-D3-003, consolidated)
**Game Affected**: All (measurement layer)
**Status**: NEW
**Location**: `crates/nif/tests/common/mod.rs` (`PerBlockHistogram::record_scene_blocks`, `compare_histograms`, `Game::mesh_archive` / `open_mesh_archive`); alias structs in `crates/nif/src/blocks/particle.rs`

Three coupled gaps in the per-block coverage test harness. Sibling of open #1841 (which tracks the *false-RED* stale-TSV data); these are the *false-GREEN* measurement-mechanism gaps — a real regression can slip the gate.

## NIF-D3-001: Per-block baseline collapses ~30 header types into alias buckets
`PerBlockHistogram`'s doc-comment says every block is keyed by its header-advertised type name and that parsed/unknown counts share a key. The code honours this only for the `NiUnknown` branch (keyed on `unknown.type_name`); the parsed branch keys on `block.block_type_name()` — the parsed struct's name. `impl_ni_object!(NiPSysBlock, …)` implicit form returns `"NiPSysBlock"` for **28** distinct header modifier types (and `"NiPSysEmitter"` for 5), collapsing them into one bucket that never appears in any NIF header table. Checked-in `per_block_baselines/fallout_76.tsv` has a single `NiPSysBlock 129380 0` row and no individual `NiPSys*Modifier` rows. Per-type regression *resolution* is lost for collapsed families (the "FO76 silently RED on NiPSysBlock" volatility).

## NIF-D3-002: `compare_histograms` only iterates baseline keys
`compare_histograms` loops `for (name, base_counts) in &baseline.counts`. A block type absent from the baseline that starts landing in `NiUnknown` has no baseline key, so neither `UnknownGrew` nor `ParsedShrank` is evaluated for it. In isolation this is compensated by `block_coverage_baselines.rs`'s `total_unknown()` ceiling — **but combined with NIF-D3-001** an offsetting alias-family swap (member A regresses to a *new* NiUnknown key, member B newly dispatches into the same bucket) leaves `total_unknown` flat and the bucket `parsed` flat, slipping **both** gates. Narrow (requires a specific double-change) but real — exactly the co-keyed invariant the doc-comment claims but the code doesn't provide.

## NIF-D3-003: Coverage baselines guard only the primary mesh archive per game
`open_mesh_archive` walks exactly one archive per game (`Game::mesh_archive` returns a single `&'static str`). Animation (`.kf`), FaceGen/geometry, texture, and DLC/patch archives are never swept. The documented sub-100% *clean* rates (FO4 96.46%, FO76 97.34%, Starfield 98.6% — FaceGen / trailing-byte tails) are file-level trailing-truncation counts, several outside the swept archive, so they are not regression-protected. This also explains the apparent non-divergence: the checked-in ceilings read `unknown_blocks 0` while the doc matrix reads <100% clean (block dispatch vs file trailing-byte truncation — different metrics). The FO4 FaceGen-morph fix (#1073) is in the swept archive and reads clean, so the doc's 96.46% may itself be stale.

## Impact
The per-type regression gate has both a resolution loss (D3-001) and a false-green hole (D3-001+D3-002) where a real NiUnknown regression can pass; the archive-scope gap (D3-003) leaves the documented truncation tails unprotected. None is a parser defect — all are test-harness measurement gaps that weaken the regression net.

## Suggested Fix
- D3-001: key the parsed histogram branch on the header-advertised name (thread it through `record_scene_blocks`, matching the doc-comment invariant) — restores per-type resolution and closes D3-002's alias half.
- D3-002: iterate the **union** of baseline and current keys in `compare_histograms` (missing baseline → `{parsed:0, unknown:0}`), flagging any newly-appearing NiUnknown type directly.
- D3-003: optionally extend `open_mesh_archive` to include the sibling archives carrying the known tails; at minimum refresh the `nif-parser.md` dispatch figure (254/310 → live **315**) and re-verify the FO4 clean rate post-#1073.

## Related
#1841 (the false-RED stale-TSV sibling), #1345 (typed-promotion that exposed the keying), `docs/engine/nif-parser.md`; project memory "FO76 silently RED on NiPSysBlock".

## Completeness Checks
- [ ] **SIBLING**: After re-keying the parsed branch, regenerate the 7-game TSVs so header-named rows replace the `NiPSysBlock` aggregate (coordinate with #1841's regen)
- [ ] **TESTS**: A harness unit test asserts a synthetic new-NiUnknown type is flagged by `compare_histograms` (pins the union-key fix)

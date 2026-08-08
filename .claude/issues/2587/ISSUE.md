# SK-D6-02: .bto/.btr distant-LOD NIFs are outside every corpus regression gate -- nif_stats filters on .nif only

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2587
**Finding ID**: SK-D6-02

**Severity**: MEDIUM
**Dimension**: Specialty Blocks + Real-Data Rendering
**Location**: `crates/nif/examples/nif_stats.rs:577,605`
**Status**: NEW

## Description
The tool backing the Meshes0/Meshes1 clean-parse baselines and the per-block/block-coverage baseline tests only considers `.nif`-suffixed archive entries. `.bto`/`.btr` are renamed NIFs through the identical `parse_nif` → `import_nif_scene` pipeline and are the entire substrate of the M35/EXAL-step-6 distant-LOD milestones — 10,662 files in `Meshes1.bsa` alone (3.3× the `.nif` count in that archive), contributing 0 to any baseline.

## Evidence
Confirmed directly: `nif_stats.rs:577,605` both filter with `.filter(|p| p.to_ascii_lowercase().ends_with(".nif"))`, no `.bto`/`.btr` inclusion. Hand-parsed this run: 10,662/10,662 clean, 0 zero-mesh — no live regression today, but nothing keeps it that way.

## Impact
A parser change breaking Skyrim distant-LOD geometry would pass the full corpus gate silently. Given SK-D6-01 (this session) already hides the *runtime consumption* of these files in 9/12 worldspaces, a parse regression on top would be invisible twice over.

## Related
SK-D6-01 (this session); NIF corpus baseline tests.

## Suggested Fix
Widen the archive-entry filter to `.nif`/`.bto`/`.btr`, re-baseline.

## Completeness Checks
- [ ] **TESTS**: Re-baselined corpus gate includes `.bto`/`.btr` entries and passes clean

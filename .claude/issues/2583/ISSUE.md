# SKY-D4-01: CLAUDE.md's documented --master repro command is wrong (fails verbatim)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2583
**Finding ID**: SKY-D4-01

**Severity**: LOW (documentation defect — the engine's own error handling did its job correctly)
**Dimension**: Multi-Master Load Order + TES5 Cell-Load Regression
**Location**: `CLAUDE.md` Usage section; this audit skill's own Dimension-4 brief cites the identical broken repro
**Status**: NEW

## Description
`cargo run -- --master Skyrim.esm --esm Dawnguard.esm --cell ForebearsHoldoutInt01` fails outright against real data: `Dawnguard.esm`'s actual `MAST` list is `["Skyrim.esm", "Update.esm"]` (the doc omits the second master), and `ForebearsHoldoutInt01` is not a real cell EditorID (the real interior is `Forelhost01`). With both corrections (`--master Skyrim.esm --master Update.esm --esm Dawnguard.esm --cell Forelhost01 --bsa …`), the cell loads cleanly: 10,045 entities, 928 meshes, 343 textures, 78.5 FPS, zero errors — proving the underlying M46.0 repeatable-`--master` FormID remap works correctly. The failure mode is soft: the engine logs one clear `ERROR` line naming the missing master but then falls back to the default 6-entity demo scene and keeps running rather than exiting non-zero, so a `--bench-hold` run not watching stderr closely could believe the repro "worked."

## Evidence
Confirmed directly: `CLAUDE.md:314` — `cargo run -- --master Skyrim.esm --esm Dawnguard.esm --cell <id> --bsa …` — single `--master`, missing the required second (`Update.esm`).

## Impact
Anyone verifying multi-master support via the documented command gets a false failure signal.

## Suggested Fix
Update the `--master` line in `CLAUDE.md`'s Usage section to `--master Skyrim.esm --master Update.esm --esm Dawnguard.esm --cell Forelhost01 --bsa …`. No code change needed — engine behavior is correct per #561's design intent.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change); manually re-verify the corrected command loads cleanly before closing

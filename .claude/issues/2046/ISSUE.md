# TD2-103: finish_partial_import never received the game-aware BSXFlags bit-5 fix — live bug on exterior-streaming path

**GitHub Issue**: #2046
**Labels**: medium,import-pipeline,legacy-compat,tech-debt,bug

**Severity**: MEDIUM (promoted: divergent bug-fix history + currently-reachable defect, not just duplication)
**Dimension**: 2 (Logic Duplication) — LIVE BUG
**Location**: `byroredux/src/cell_loader/partial.rs:54-59` vs. `byroredux/src/cell_loader/references/import.rs:69-100`

## Description
Both functions gate NIF import on BSXFlags bit 5. `references/import.rs` (sync REFR path) carries the fix: on Skyrim+/FO4/FO76/Starfield, bit 5 means `MultiBoundNode`, not "editor marker" — treating it as editor-marker on those games silently drops legitimate architecture (15 FO4 NIFs per the fix commit). `partial.rs` — the main-thread drain for the exterior-streaming worker, reachable on every game's exterior-cell streaming path — still applies the pre-fix unconditional gate. Its `PartialNifImport` struct never gained a `bsver` field, so it's structurally unable to apply the fix even if copied.

## Evidence
`references/import.rs:91`: `let bsx_editor_marker = bsx & 0x20 != 0 && bsver < byroredux_nif::version::bsver::FALLOUT4;` (game-era gated). `partial.rs:54`: `if partial.bsx & 0x20 != 0 { ... skip unconditionally ... }` — no `bsver` gate exists, confirmed by reading both functions directly; `partial.rs`'s `PartialNifImport` struct carries no `bsver` field to gate on even if the check were added inline.

## Impact
Any FO4/Skyrim exterior cell streamed through the async worker path (the default exterior-loading path) can still silently drop architecture NIFs with bit 5 set.

## Related
Fix commit `6feac029`; #1215 (zero-contribution warning, closed, also missing from `partial.rs`).

## Suggested Fix
Extract a shared `build_cached_nif_import(scene, bsx, bsver, ...)` helper covering the gate + import + BGSM merge + zero-contribution warning, called from both paths; thread `bsver` through `PartialNifImport`/the streaming payload chain.

**Effort**: medium

## Completeness Checks
- [ ] **SIBLING**: Confirm no other streaming/async NIF-import path (besides `partial.rs`) independently re-implements the BSXFlags bit-5 gate without the game-era check
- [ ] **TESTS**: A regression test pins this specific fix — e.g. an FO4 exterior-streaming test asserting `hitfloorsolidfull01.nif`-class BSXFlags=0xA2 architecture is NOT dropped via the async worker path

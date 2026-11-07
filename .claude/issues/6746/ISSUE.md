# REN-D6-01: doc rot left by the #6712 constant move in `asset_provider/material/mod.rs` and `NEAR_MIRROR_NEUTRAL_ROUGHNESS`

Labels: low,nifal,renderer,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6746

**Severity**: LOW · **Dimension**: NIFAL Material

## Location
`byroredux/src/asset_provider/material/mod.rs` (the `/// #951 / SAFE-26 - bounded-cache caps` block above a `use` item); `byroredux/src/material_translate.rs` (`NEAR_MIRROR_NEUTRAL_ROUGHNESS` doc)

## Description
`df17d9dba` left the three-line SAFE-26 cache-cap doc comment attached to a `use` item; the caps it describes (`MAX_FAILED_PATHS`, `MAX_BGEM_CACHE_ENTRIES`, ...) live in `provider.rs` with their own docs. Separately the moved doc says the constant "Matches the neutral `classify_pbr_keyword`'s arms already use", but the classifier's no-signal terminal is `PbrMaterial::NO_SIGNAL_NEUTRAL.roughness == 0.85` (since #5197) while the constant is 0.5 (it equals only the skin/body arm).

## Evidence
`grep -n 'SAFE-26' byroredux/src/asset_provider/material/mod.rs` shows one hit at the orphaned doc; `NO_SIGNAL_NEUTRAL` in `crates/core/src/ecs/components/material.rs` is `{ roughness: 0.85, metalness: 0.0 }`; the constant is `pub(crate) const NEAR_MIRROR_NEUTRAL_ROUGHNESS: f32 = 0.5;`. The move itself was verbatim (`material_flags.rs` is a rename, 0 content diff).

## Impact
Documentation only. Whether 0.5 vs 0.85 is intended is NIFAL-owned (`/audit-nifal`); this finding claims only that the sentence is false as written.

## Suggested Fix
Delete the orphaned SAFE-26 lines; reword the constant doc to say it is a chosen mid-roughness, or decide it should be `NO_SIGNAL_NEUTRAL.roughness` (an owner call).

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Same wording checked in related files (docs, other comments)
- [ ] **TESTS**: Covered by existing guards / `.claude/commands/_audit-validate.sh`; no new test needed for pure comment edits

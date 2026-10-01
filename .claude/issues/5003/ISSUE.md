# Issue #5003

**Title:** SF-2026-09-29-D3-01: #4429's XOR guard counts any backticked mention anywhere in nifal.md as "parked"
**State:** OPEN
**Labels:** bug, import-pipeline, low, legacy-compat, game:starfield, test-gap

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-29.md`
**Severity**: LOW
**Dimension**: CDB Material Database (test coverage)
**Location**: `byroredux/src/asset_provider/tests/starfield_mat.rs` (`starfield_single_channel_kinds_are_parked_by_name_or_canonical`); `docs/engine/nifal.md` (Starfield parked-kinds paragraph)

## Description
The guard's doc says it fails on "neither", i.e. a kind dropped from the parked table without a struct field. But `parked` is `NIFAL_SRC.contains(&format!("`{suffix}`"))` over the whole of nifal.md. The "parked table" is one prose paragraph, and four of the five suffixes appear in nifal.md more than once (kind list, TXST slot map, `smooth_spec` sign-flip rationale). Mention counts at HEAD: `_rough` 3, `_metal` 2, `_ao` 2, `_opacity` 2, `_transmissive` 1.

## Evidence
If Phase 2 edits the kind list to drop `_rough` without adding `pub roughness: T,`, the guard stays green, because the TXST map line (`TX09 → `_rough``) and the rationale line (`routing `_rough` there`) still satisfy `contains`. `_transmissive` is the only kind pinned as described.

## Impact
Test coverage only; no runtime effect today (zero Starfield roles are produced). The guard exists to stop CDB Phase 2 from quietly dropping ~39% of Starfield's texture kinds, and for four of five kinds it cannot see a drop from the list.

## Related
#4429 (closed), #3398 (CDB Phase 2), SF-2026-09-16-D3-01.

## Suggested Fix
Anchor the needle on the parked list itself: put the five kinds in a delimited span (e.g. a `<!-- parked-starfield-kinds -->` fenced block or a table) and scan only that span, as the test already narrows `types.rs` to the `MaterialTextureSet` body.

Validated at HEAD 9fcfdc3fc: the test computes `parked = NIFAL_SRC.contains(...)` over the whole `include_str!` of nifal.md; backticked-suffix counts in nifal.md are 3/2/2/2/1 as stated.

## Completeness Checks
- [ ] **SIBLING**: other `include_str!` doc-scan guards that match on the whole file rather than an anchored span
- [ ] **TESTS**: The tightened guard is shown to fail when a kind is removed from the anchored list


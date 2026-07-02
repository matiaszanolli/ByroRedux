# 1826: FO4-D4-01: parse_sub_index recovery can desync the stream when block_size is None

URL: https://github.com/matiaszanolli/ByroRedux/issues/1826
Labels: bug, nif-parser, low, legacy-compat

**Severity**: LOW
**Dimension**: 4 — NIF BSVER 130
**Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:639-654`
**Status**: NEW (latent, unreachable on current dispatch)

## Description

When `BsSubIndexTriShapeData::parse` errors AND the caller passed `block_size
== None`, the recovery only does `stream.set_position(segmentation_start)`
(`:653`) — a bare rewind with no compensating forward skip, so a subsequent
block read could start inside this block's segmentation payload. The
`Some(size)` arm (`:639-651`) computes a proper skip target and re-skips if
needed. The live dispatcher (`crates/nif/src/blocks/mod.rs:466-478`) always
passes `Some(_)` (block sizes appear in the header since V20.2.0.5), so the
`None` path is currently unreachable in practice.

## Evidence

- `Some(size)` arm computes a skip target and re-skips on overshoot (`bs_tri_shape.rs:639-651`).
- The `else` arm (`None` case) only rewinds: `stream.set_position(segmentation_start);` (`:653`), with no compensating skip.
- Live dispatch always supplies `Some(_)`: `crates/nif/src/blocks/mod.rs:466-478`.

## Impact

None today (unreachable on the current dispatch). If a future size-less caller
hits this on a decode failure, the outer block-loop's `block_size` resync still
fires afterward so overall parse rate is unaffected — but a debug breadcrumb
at that point would point at a misleading stream offset.

## Suggested Fix

If `parse_sub_index` ever gains a size-less caller, make the `None` arm return
`Err` instead of a bare rewind, so the outer loop's `block_size` recovery
remains the single source of truth for resync.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other `BsTriShapeKind` recovery arm has the same size-less bare-rewind gap.
- [ ] **TESTS**: A regression test covers the `block_size == None` decode-failure path once a caller can exercise it (or a `debug_assert!(block_size.is_some())` documents the current invariant explicitly).


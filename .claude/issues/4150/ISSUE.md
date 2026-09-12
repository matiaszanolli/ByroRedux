# NIF-D1-2026-09-11-02: Unknown BoundVolumeType continues without consuming its body

URL: https://github.com/matiaszanolli/ByroRedux/issues/4150
Labels: bug, nif-parser, medium, nif

---

**Severity**: MEDIUM
**Dimension**: 1 — Stream Position Integrity
**Game Affected**: Pre-Gamebryo content, no `block_sizes` recovery anchor
**Location**: `crates/nif/src/blocks/base.rs:328-334`
**Status**: NEW (carry-forward of NIF-2026-09-04-D1-04; no matching GitHub issue)

**Description**: The wildcard match arm for an unrecognized `bv_type` in `read_and_skip_bounding_volume` logs "skipping" but performs no `stream.skip(...)` — the stream is left mid-body with no bytes consumed.

**Evidence** (`crates/nif/src/blocks/base.rs:328-334`):
```rust
_ => {
    log::warn!("Unknown bounding volume type {}, skipping", bv_type);
}
```
Confirmed: no `stream.skip()` call in this arm, unlike every other arm (SPHERE/BOX/CAPSULE/HALF_SPACE all call `stream.skip(N)`).

**Impact**: On the no-`block_sizes` band, a wrong `bv_type` silently misaligns every subsequent field in the file with zero reconciliation to catch it.

**Suggested Fix**: Return `Err(InvalidData)` on the unrecognized-type arm so the caller's existing recovery paths engage.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the corrected error-return behavior for an unknown `bv_type`


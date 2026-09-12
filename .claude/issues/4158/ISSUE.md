# NIF-D1-2026-09-11-03: skip_animation's block-size skip escapes as a whole-parse Err instead of engaging truncation recovery

URL: https://github.com/matiaszanolli/ByroRedux/issues/4158
Labels: bug, nif-parser, low, nif

---

**Severity**: LOW
**Dimension**: 1 — Stream Position Integrity
**Location**: `crates/nif/src/lib.rs:459-469`
**Status**: NEW

**Description**: `skip_animation`'s block-size skip escapes as a whole-parse `Err` via `?` instead of engaging the loop's normal truncation recovery.

**Evidence** (`lib.rs:459-469`):
```rust
if options.skip_animation && is_animation_block(type_name) {
    if let Some(size) = block_size {
        stream.skip(size as u64)?;
        ...
```
The `?` here propagates any skip failure straight out of the whole parse, bypassing the recovery path used elsewhere in the loop for a bad `block_size`.

**Impact**: A truncated/corrupt animation block under `skip_animation` mode fails the entire file parse instead of degrading gracefully like other malformed blocks.

**Suggested Fix**: Route the skip failure through the same truncation-recovery path (`NiUnknown` substitution + `recovered_blocks` counter) used elsewhere in the dispatch loop instead of `?`.

## Completeness Checks
- [ ] **TESTS**: A fixture with a truncated animation block under `skip_animation: true` pins the graceful-degradation behavior


# NIF-D1-2026-09-11-04: block_size-driven realignment can park the cursor past EOF silently

URL: https://github.com/matiaszanolli/ByroRedux/issues/4159
Labels: bug, nif-parser, low, nif

---

**Severity**: LOW
**Dimension**: 1 — Stream Position Integrity
**Location**: `crates/nif/src/lib.rs:536,629`
**Status**: NEW

**Description**: `block_size`-driven realignment (`stream.set_position(start_pos + size as u64)`) can park the cursor past EOF silently, unlike `stream.skip()` which bounds-checks.

**Evidence** (`lib.rs:536` and `:629`, both `stream.set_position(start_pos + size as u64);` following a declared `block_size`).

**Impact**: A `block_size` value that overruns the actual file length silently parks the cursor past EOF instead of surfacing a clear error, deferring the failure to whatever the next read attempts.

**Suggested Fix**: Bounds-check the target position against the stream length before calling `set_position`, mirroring `skip()`'s existing bounds check, and return `Err` on overrun.

## Completeness Checks
- [ ] **TESTS**: A fixture with an oversized `block_size` value pins the corrected bounds-checked behavior


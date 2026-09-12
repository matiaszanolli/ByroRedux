# NIF-D2-2026-09-11-02: NiTexturingProperty.Apply Mode gates on STRING_TABLE_THRESHOLD, a constant reserved for header/stream code

URL: https://github.com/matiaszanolli/ByroRedux/issues/4152
Labels: bug, nif-parser, medium, nif, game:oblivion

---

**Severity**: MEDIUM
**Dimension**: 2 — Version Gating
**Game Affected**: Oblivion (~30,121 `NiTexturingProperty` instances)
**Location**: `crates/nif/src/blocks/properties.rs:285`; `crates/nif/src/version.rs:165-174`
**Status**: NEW (re-verified carry-forward of NIF-D2-2026-09-04-02; code unchanged since, no matching GitHub issue)

**Description**: `NiTexturingProperty.Apply Mode`'s read shape gates on `STRING_TABLE_THRESHOLD` (`stream.version() <= NifVersion::STRING_TABLE_THRESHOLD`). Numerically correct today (same cut point as the real `Apply Mode` version condition), but `STRING_TABLE_THRESHOLD`'s own doc comment declares a lockstep contract scoped to `header.rs`/`stream.rs` only ("MUST be kept in lockstep across header.rs::NifHeader::parse and stream.rs::NifStream"). `properties.rs` is an undeclared third consumer of a constant meant for an unrelated concept (the per-file string table cutover).

**Evidence** (`properties.rs:283-288`):
```rust
let apply_mode = if stream.version() < NifVersion::V3_3_0_13 {
    APPLY_MODULATE
} else if stream.version() <= NifVersion::STRING_TABLE_THRESHOLD {
    stream.read_u32_le()?
} else {
    u32::from((flags >> 1) & 0x7)
};
```

**Impact**: Latent — a future coordinated rename/renumber of the constant (scoped only to header/stream lockstep per its own doc) silently flips `Apply Mode`'s read shape with no recovery anchor below v20.2.0.5.

**Suggested Fix**: Add a distinct `V20_1_0_1`-scoped name for this call site instead of reusing `STRING_TABLE_THRESHOLD`.

## Completeness Checks
- [ ] **TESTS**: A two-sided test pins `Apply Mode`'s read shape independent of the string-table constant


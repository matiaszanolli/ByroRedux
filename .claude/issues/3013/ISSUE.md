# SCR-D8-2026-08-16-02: an out-of-range track_to_bone entry silently drops a whole animation track — nothing validates the Havok binding against the skeleton

Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-16.md` (Dimension 8 — Havok packfile reader).

**Location**: `crates/hkx/src/animation.rs`:243-262 (the binding decode)

## Description

`track_to_bone` is decoded from the packfile as raw `u16` values. The decode validates that `binding_count == transform_count`, but **nothing validates each entry against the skeleton's bone count**. An out-of-range entry silently drops a whole animation track.

## Evidence

```rust
// crates/hkx/src/animation.rs:255-261
let raw = pack.data_slice(offset, binding_count * 2, "transform bindings")?;
raw.chunks_exact(2)
    .map(|bytes| Ok(u16::from_le_bytes(bytes.try_into().unwrap())))
    .collect::<Result<Vec<_>>>()?
```

Re-verified 2026-08-17: the only check is the count agreement above it (`binding_count != transform_count → InvalidData`). Each `u16` is accepted as-is; the skeleton is never consulted.

## Impact

A binding that names a bone the skeleton does not have causes its track to be dropped at bind time — the animation plays with that bone unanimated, with no diagnostic. On a partially-mismatched skeleton this degrades silently rather than failing, which makes it hard to distinguish from an authoring problem.

## Suggested Fix

Validate each `track_to_bone` entry against the decoded skeleton's bone count at bind time and either reject the clip (`InvalidData`) or log the dropped track explicitly. Silent dropping is the part worth removing.

## Related

- #3011 (SCR-D8-2026-08-16-01), SCR-D8-2026-08-16-03 (#3018), SCR-D8-2026-08-16-04 (#3014) — same crate, same parser-discipline gap

## Completeness Checks
- [ ] **SIBLING**: The `binding_count == 0` identity branch checked for the same overflow risk
- [ ] **NOT-SILENT**: A dropped track logs or errors — never disappears quietly
- [ ] **SKELETON-AWARE**: The validation consults the actual decoded skeleton, not a constant
- [ ] **TESTS**: A negative-input test binds an out-of-range bone index and asserts the chosen behaviour

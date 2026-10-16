# #5895: PAR-D2-2026-10-15-03: `read_bone_weights` computes `offset - 1` on a file-controlled `i32`

- **Labels**: medium,bug,animation
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: `read_bone_weights` computes `offset - 1` on a file-controlled `i32`. A binding's `offsetInArrayPlusOne = 0x80000000` panics with overflow in dev builds, on the main thread, with no `catch_unwind`.

- **Severity**: MEDIUM (a panic reachable from a mod file in the engine's default dev profile, which has overflow checks on; release wraps and drops the binding)
- **Dimension**: Error Semantics
- **Location**: `crates/hkx/src/behavior.rs:1074-1075`
  ```rust
  let offset = file.u32(element + BINDING_OFFSET_IN_ARRAY, "binding array offset")? as i32;
  if let (Ok(variable), Some(bone)) = (usize::try_from(variable), usize::try_from(offset - 1).ok()) {
  ```
- **Status**: NEW
- **Description**:
  - When the raw u32 is 0x80000000, the cast gives `offset = i32::MIN`, and `offset - 1` overflows.
  - The decode callers listed in D1-01 have no `catch_unwind`. The only HKX-adjacent one is in `streaming/pre_parse.rs` and covers NIF only. A panic here kills the dev engine during cell load.
  - This is the same class as #5008 (a file-controlled panic that only debug builds hit).
- **Trigger Input**: an `hkbBoneWeightArray` binding whose member path is `boneWeights` and whose `offsetInArrayPlusOne` field is 0x80000000.
- **Suggested Fix**: `offset.checked_sub(1).and_then(|o| usize::try_from(o).ok())`. Better, read the field as `u32` and use `checked_sub(1)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix

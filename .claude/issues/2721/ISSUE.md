# #2721: Three live "100-byte Vertex" doc sites; the test-pinned value is 104

- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `docs/engine/ui.md:271`, `docs/engine/testing.md:88`, `crates/renderer/src/vulkan/pipeline.rs:806`
- **Status**: NEW
- **Description**: All three describe the scene `Vertex` as 100 bytes. The
  live value is **104**, pinned by `crates/renderer/src/vertex.rs:331`
  (`assert_eq!(size_of::<Vertex>(), 104)`), and has been since the RGBA-color
  widening in `cd2b5fe4`. The sibling doc comment at `vertex.rs:278` correctly
  says 104, so the *same crate* now disagrees with itself.
- **Evidence**:
  - `docs/engine/ui.md:271` — "rather than the full 100-byte scene `Vertex`"
  - `docs/engine/testing.md:88` — "`vertex.rs` pins the 100-byte stride / 9 attribute descriptions"
  - `crates/renderer/src/vulkan/pipeline.rs:806` — "instead of the full 100-byte Vertex (post-M-NORMALS, #783)"
  - `crates/renderer/src/vertex.rs:278` — "Using this instead of the full 104-byte `Vertex` (post-" ✅
- **Impact**: A wrong number in a vertex-input layout contract. The
  `UiVertex`-vs-`Vertex` split these three sites explain exists *because* of
  the size delta, so the stated rationale is quantitatively wrong. Anyone
  sizing a staging buffer or reasoning about skinned-vertex stride from the
  docs gets a 4-byte-per-vertex error.
- **Related**: Promoted per the severity table's "stale GPU-struct size in a
  doc comment (lockstep-drift bait)" trigger — `Vertex` is not literally in
  that row's `GpuCamera`/`GpuInstance`/`GpuMaterial` list, but it is the same
  class of test-pinned `#[repr(C)]` layout contract. The **CLAUDE.md** instance
  of this same stale number was found by `AUDIT_PERFORMANCE_2026-07-25` (D6-02)
  and fixed; that pass did not sweep for siblings, and these three survived.
  Same doc-rot class as today's #2696 / #2703, different subsystem.
- **Suggested Fix**: `100` → `104` at all three sites. Then grep
  `100-byte\|100 byte` once more — the remaining hits
  (`crates/plugin/src/esm/records/items.rs:205`, `crates/nif/src/header.rs:575`,
  `crates/nif/src/blocks/dispatch_tests/havok.rs:33`) are unrelated record/block
  sizes and are correct.
- **Effort**: trivial

---
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-12.md` (finding `TD3-2026-08-12-01`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)


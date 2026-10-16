# #5901: PAR-D3-2026-10-15-01: `read_reference_frame` hard-codes 64-bit field offsets (`DURATION 0x40`, `SAMPLES 0x48`) while the rest of the v11 clip path derives offsets from the pointer size, so a 32-bit Havok 2014 clip reads the wrong fields silently.

- **Labels**: low,bug,animation,game:fo4
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

- **Severity**: LOW (no vanilla 32-bit v11 file exists, but `Packfile::parse` accepts the combination)
- **Dimension**: Version Gating
- **Location**:
  - `crates/hkx/src/animation.rs:640-648`
  - Contrast `Layout::new(pack.pointer_size())` at `:399`, and `Packfile::parse` accepting pointer size 4 together with version 11 (`packfile.rs:55-90`)
- **Status**: NEW
- **Description**:
  - On a 32-bit packfile, `hkReferencedObject` is 8 bytes. That moves `m_up`, `m_forward`, `m_duration` and `m_referenceFrameSamples` to 0x10 / 0x20 / 0x30 / 0x34.
  - The reader instead uses the 64-bit 0x40 / 0x48. It reads bytes from inside the forward vector as the duration and the sample count, and may return `Some` with a garbage path.
  - The behavior reader refuses 32-bit (`require_64_bit`). The clip reader does not.
- **Trigger Input**: a 32-bit (`bytes[0x10] == 4`) `hk_2014` clip with a reference frame.
- **Suggested Fix**: derive both offsets from `Fields::referenced_object(pointer)` (frame-type byte, then two 16-aligned vectors, an int and an array), or return `None` with a warning when `pointer_size() != 8`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix

# #6306: PAR-D1-03: The lossless decoder does not tie `frame_count` to the file's bytes when every channel is absent or static, and it accepts a zero duration over many frames. The spline path refuses both.

**Labels**: low,bug,import-pipeline,animation,game:fo4

**Source**: docs/audits/AUDIT_PARSERS_2026-10-21.md · **Severity**: LOW · **Dimension**: Size Discipline · **Location**: `crates/hkx/src/animation.rs` (`decode_lossless_in`)

- **Location**: `crates/hkx/src/animation.rs:822-865`, `891`, `942`
- **Status**: NEW (in code added by 2649febc7)
- **Trigger Input**: a lossless clip with `transform_count = 4,096`, every descriptor 0 (absent), and `m_numFrames = 244`. The file is about 41 KB: the descriptor tables only.
- **Description**:
  - Dynamic series must fill `series × frame_count` entries, which ties output to the file. Absent and static channels cost nothing per frame.
  - A file of a few tens of KB can therefore claim up to `MAX_TRANSFORM_SAMPLES` (1 M) samples. That is about 40 MB of `HkxTransform`, and about 120 MB of key structs retained by `convert_hkx_clip`.
  - The spline path closed exactly this gap with #4655 and #5006 (frames ≤ blocks × (frames per block − 1) + 1).
  - Separately, `duration = 0` with `frame_count > 1` is accepted (`frame_duration` 0, every key at t = 0). The spline path rejects `frame_duration <= 0`.
- **Suggested Fix**:
  - Bound `frame_count` by the dynamic data present. For example, with no dynamic series, require `frame_count` ≤ a small constant, or charge the absolute cap against `transform_count`.
  - Reject `duration <= 0` when `frame_count > 1`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

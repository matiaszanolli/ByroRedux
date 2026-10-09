# #5457: SAFE-D4-2026-10-08-01: `load_shader_module`'s SAFETY text still cites "the `chunks_exact(4)` decode above", which #5308 rewrote to `as_chunks::<4>()`

**Labels**: low,safety,renderer,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5457

**Source**: `docs/audits/AUDIT_SAFETY_2026-10-08.md` — `SAFE-D4-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW. This is doc rot inside a safety justification. The invariant is still true: the decode builds an owned `Vec<u32>`, which is u32-aligned by construction, and `spv.len().is_multiple_of(4)` is asserted first.
- **Dimension**: Unsafe-Block Discipline
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:24-37`. The decode is at `:24-28` and the SAFETY comment at `:33-36`.
- **Status**: NEW. #5308 (CLOSED, `b24cb46b6`) changed the code and missed the comment. Its three sibling rewrites (`context/depth_capture.rs:100`, `context/screenshot.rs:91`, `groundcover/frame.rs:183`) have no SAFETY text that names the old API.
- **Description**: The skill states that the audit's value is the truth of each SAFETY claim. This one now justifies alignment by a call that no longer exists in the function. The next reader who checks the claim will not find it.
- **Evidence**:
  ```rust
  let code: Vec<u32> = spv
      .as_chunks::<4>().0
      .iter()
      .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
      .collect();
  ...
  // SAFETY: `device` is the live logical device; `create_info` borrows
  // `code`, which outlives this call, and the SPIR-V was 4-byte aligned
  // by the `chunks_exact(4)` decode above; ...
  ```
- **Impact**: None at runtime. It is a misleading justification on an FFI call.
- **Related**: #5308.
- **Suggested Fix**: Reword the comment to "`code` is an owned `Vec<u32>` (u32-aligned), decoded from a length asserted to be a multiple of 4". This states the real invariant rather than naming an API.

## Also reported as `REN-D5-2026-10-08-03` (AUDIT_RENDERER_2026-10-08.md)

Cross-report duplicate merged at publish time; the sibling report's text follows.

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-08.md` — `REN-D5-2026-10-08-03` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/pipeline.rs`, `load_shader_module`, the `SAFETY` comment: "the SPIR-V was 4-byte aligned by the `chunks_exact(4)` decode above".
- **Status**: NEW (residue of `b24cb46b6`, #5308).
- **Description**: the decode is `spv.as_chunks::<4>().0`. The behaviour is identical (the remainder is dropped either way, and the function asserts `spv.len().is_multiple_of(4)` first), but the comment now names a call that no longer exists.
- **Suggested Fix**: reword to name the `is_multiple_of(4)` assert and `as_chunks`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other SAFETY comments touched by #5308's `chunks_exact` → `as_chunks` rewrite)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant

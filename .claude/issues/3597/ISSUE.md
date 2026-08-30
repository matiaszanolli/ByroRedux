# #3597 — REN-2026-08-30-D9-02: `SkinComputePipeline::dispatch`'s SAFETY comment still describes the pre-#3231 12-byte push block and names a test that no longer exists

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3597 --json state`.

---

- **Severity**: LOW (doc-rot)
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs:680-684` (`SkinComputePipeline::dispatch`)
- **Status**: OPEN — new
- **Description**: The SAFETY justification for the `std::slice::from_raw_parts` that builds the push-constant byte view asserts a struct shape that #3231 changed 3 fields ago, and cites a test name that is not in the tree. `SkinPushConstants` (`skin_compute.rs:48-74`) is now `u64, u64, u32, u32, u32, u32` = 32 B; the live pin is `push_constants_size_is_32_bytes` (`skin_compute.rs:1177`). The sibling palette dispatch's comment (`skin_compute.rs:1029-1031`) is accurate, which makes the drift a local one rather than a house-style issue.
- **Evidence**:
  - `skin_compute.rs:680-684`: `// SAFETY: `SkinPushConstants` is `repr(C)` with three u32 fields, / 12 bytes, no interior padding. … mismatched shape is caught by `push_constants_size_is_12_bytes` test).`
  - `grep -n "push_constants_size_is" crates/renderer/src/vulkan/skin_compute.rs` → only `push_constants_size_is_32_bytes` (1177) and `skin_palette_push_constants_size_is_4_bytes` (1524). No `_12_bytes` test exists.
  - `skin_compute.rs:1177-1187` asserts `PUSH_CONSTANTS_SIZE == 32` and `size_of::<SkinPushConstants>() == 32`.
- **Impact**: No runtime effect — the code takes `PUSH_CONSTANTS_SIZE`, not the literal, and the shader block (`skin_vertices.comp:92-110`) matches at 32 B. The cost is that the SAFETY argument on an `unsafe` block is unverifiable as written and points a reader at a non-existent guard, exactly the failure mode the "verify the premise" rule exists for. A future editor checking the invariant finds nothing and may conclude it is unpinned.
- **Suggested Fix**: Rewrite the comment to describe the current layout (two `u64` at offsets 0/8, four `u32` at 16/20/24/28, 32 B, no interior padding) and cite `push_constants_size_is_32_bytes`. One-line change; no test needed beyond the one already there.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D9-02

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review

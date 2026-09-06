# #4048 — REN-2026-09-06-D9-02: `SkinPushConstants`'s own doc comment still describes the pre-#3231 12-byte / three-`u32` block

**Labels**: low, renderer, shaders, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D9-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW (doc-rot)
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs` (the doc comment on `pub struct SkinPushConstants`)
- **Status**: NEW — sibling site of the now-fixed `REN-2026-08-30-D9-02` (that finding named
  the SAFETY comment on the `from_raw_parts` inside `SkinComputePipeline::dispatch`, which
  *has* been corrected to "six fields (u64, u64, u32, u32, u32, u32), 32 bytes"; the struct's
  own header was not updated in the same pass). Verified against code, not GitHub — no open
  issue matches `push_const|SkinPushConstants`.
- **Description**: The doc block immediately above the struct reads "**12 bytes (3 × u32).**
  std430 doesn't require 16-B block alignment when no vec4 follows, so we ship the tight
  layout." The struct beneath it has had six fields since #3231 and measures 32 B. The
  next paragraph *within the same struct*, on the `morph_delta_address` field, correctly
  explains the u64-first ordering and cites #3231 — so the header contradicts its own
  field docs three lines later.
- **Evidence**: `PUSH_CONSTANTS_SIZE` = `size_of::<SkinPushConstants>()`; both are pinned at
  32 by `push_constants_size_is_32_bytes`, whose body itself narrates the 12 → 32 growth.
  The GLSL `PushConstants` block in `skin_vertices.comp` ends with the comment "32 B total,
  matches Rust `SkinPushConstants` exactly".
- **Impact**: None at runtime — every consumer takes `PUSH_CONSTANTS_SIZE`, never the
  literal. The cost is that the one comment a reader hits *first* when opening this struct
  states a wrong number in a CPU/GPU layout contract, which is the failure mode the
  audit-common symbol-advisory rule exists to catch.
- **Related**: `REN-2026-08-30-D9-02` (the sibling comment, fixed), #3231.
- **Suggested Fix**: Replace "12 bytes (3 × u32)" with "32 bytes (2 × u64 at offsets 0/8,
  4 × u32 at 16/20/24/28; no interior or trailing padding)" and keep the existing
  128 B-minimum sentence. One line; already covered by `push_constants_size_is_32_bytes`.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix

# REN-D5-03: byte_view doc claims no crate-wide hand-rolled from_raw_parts

- **Issue**: #6349 (snapshot as filed; GitHub is authoritative for live state)
- **Labels**: low, renderer, memory, documentation, doc-rot
- **Source**: docs/audits/AUDIT_RENDERER_2026-10-24.md

- **Severity**: LOW (doc drift; the cast is sound)
- **Dimension**: Memory/Lifecycle (byte views)
- **Location**: the `byte_view` doc and `create_device_local_buffer<T: NoUninit>` in `crates/renderer/src/vulkan/buffer.rs`
- **Status**: NEW (residue of #6270, which added the "holds crate-wide" sentence)
- **Description**: #6270 extended the `byte_view` doc to say the no-hand-rolled-`from_raw_parts` claim holds crate-wide except for `hash_indirect_slice`. Production code has a second one: in `create_device_local_buffer`, `let bytes: &[u8] = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data)) };` behind a `T: NoUninit` bound — sound, and exactly a `byte_view` call. The only pin, `descriptors_hash_views_route_through_byte_view_except_the_ash_type`, scans `descriptors.rs` alone, so nothing enforces the crate-wide wording.
- **Evidence**: `grep -rn from_raw_parts crates/renderer/src` finds `buffer.rs` (`byte_view` itself and `create_device_local_buffer`), `scene_buffer/descriptors.rs` (`hash_indirect_slice`) and `shader_contract_tests.rs` (test text).
- **Related**: #6270, #4445, #4521.
- **Suggested Fix**: replace the cast with `byte_view(data)`; optionally pin the crate-wide count (`byte_view` + `hash_indirect_slice` = 2 production `from_raw_parts`).


## Completeness Checks
- [ ] **UNSAFE**: removing the hand-rolled cast leaves no new `unsafe`; the `NoUninit` bound is the upheld invariant
- [ ] **TESTS**: optional crate-wide count pin (`byte_view` + `hash_indirect_slice` = 2 production `from_raw_parts`)

_Filed from `docs/audits/AUDIT_RENDERER_2026-10-24.md` (static review; no engine or Vulkan device was launched). Failures invisible to `cargo test` are flagged NEEDS-RENDERDOC in the report._

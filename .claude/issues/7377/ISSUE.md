# #7377 — REN-D3-2026-11-23-03: `INSTANCE_FLAG_LOD_BLOCK` (bit 9) and `INSTANCE_FLAG_MOVED_THIS_FRAME` (bit 10) are declared twice and never compared

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_RENDERER_2026-11-23.md` (finding REN-D3-03). Reported by reading: nothing here was run on a GPU.

- **Severity**: LOW (test gap; values agree today)
- **Dimension**: GPU-Struct Layout
- **Location**: `instance_flag_bits_match_scene_buffer_consts` (`shader_constants.rs`) vs `INSTANCE_FLAG_*` in `shader_constants_data.rs` and `scene_buffer/constants.rs`
- **Status**: NEW
- **Description**: the CPU packs `GpuInstance.flags` from `scene_buffer/constants.rs` (`build_and_upload_instances.rs::instance_flags` uses `INSTANCE_FLAG_LOD_BLOCK` and `INSTANCE_FLAG_MOVED_THIS_FRAME`), while the shaders read the generated `#define` from `shader_constants_data.rs`. The pairing test lists `NON_UNIFORM_SCALE`, `ALPHA_BLEND`, `CAUSTIC_SOURCE`, `TERRAIN_SPLAT`, `FLAT_SHADING`, `DIFFUSE_ALPHA` and (new, `d4fcb4c69`) `TERRAIN_PATCH`; the two flags above are absent. `constants.rs`'s own table proves only that each is a single bit and all are disjoint. `MOVED_THIS_FRAME` drives the ReSTIR mover taint (`#6250`), `LOD_BLOCK` the distant-LOD shading path.
- **Evidence**: `grep -rn "INSTANCE_FLAG_LOD_BLOCK\|INSTANCE_FLAG_MOVED" crates/renderer/src` shows both definitions (1 << 9 / 1 << 10 in each file) and no `assert_eq!` between them.
- **Impact**: editing one file's bit silently splits CPU and GPU: motion taint never fires, or LOD blocks shade as plain meshes. Nothing fails.
- **Related**: #4031 (earlier mirror-count premise), #6250.
- **Suggested Fix**: two `assert_eq!` lines in `instance_flag_bits_match_scene_buffer_consts`; better, make `scene_buffer/constants.rs` re-export the data file's values so there is one declaration.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other passes, other shaders, other call sites)
- [ ] **TESTS**: A regression test pins this specific fix

# #4954: REN-D3-2026-09-27-03: `upload_lights` now runs a std SipHash `HashMap` and a `DefaultHasher` over a 4 KiB remap every frame (hot-path hashing rule)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4954
- **Labels**: low,renderer,performance,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D3-2026-09-27-03**._

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (upload path) / performance
- **Location**: `crates/renderer/src/vulkan/scene_buffer/light_history.rs:4,14` (`use std::collections::HashMap`; `scratch: HashMap<Identity, (u32, u32, u32, u32)>`); `crates/renderer/src/vulkan/scene_buffer/upload.rs:121-126` (`SceneBuffers::upload_lights`: `std::collections::hash_map::DefaultHasher` over `hash_light_slice(..)` plus `previous_to_current`)
- **Status**: NEW
- **Description**: `_audit-common.md`'s hot-path rule (#2923) requires the per-frame render path to use `FxHashMap`/`FxHashSet` end to end. `LightHistory::remap` runs every frame from `upload_lights`. It clears and refills a std `HashMap` keyed by `[u32; 4]`, with up to 1023 inserts and 1023 lookups at SipHash-1-3 cost.
  - The dirty-gate hash was previously a single `FxHasher::write` in `hash_light_slice`. It now wraps that in a SipHash `DefaultHasher` and feeds it the whole `[u32; 1024]` remap array (4 KiB) through `Hash`.
  - The remap array is also returned by value (4 KiB copy) and copied again into `LightHeader`.
- **Evidence**: `light_history.rs` imports `std::collections::HashMap`, while `descriptors.rs` `hash_light_slice` uses `rustc_hash::FxHasher`. `upload.rs:123` shows `let mut hasher = std::collections::hash_map::DefaultHasher::new();`.
- **Impact**: This is a small, bounded CPU cost per frame, not a correctness problem. It is the exact regression class the rule names, on a path that runs every frame.
- **Related**: #2923, #2036 (the light dirty-gate).
- **Suggested Fix**: Use `rustc_hash::FxHashMap` for `scratch`. Fold the remap into the gate with `FxHasher::write(bytemuck-style byte view of previous_to_current)` rather than `DefaultHasher` + `Hash`.
- **Also reported by**: Dim 4 (as REN-D4-2026-09-27-03, owner note: `/audit-performance`) and Dim 5 (as REN-D3-2026-09-27-03). Merged here; orchestrator confirmed `use std::collections::HashMap` in `scene_buffer/light_history.rs` and `std::collections::hash_map::DefaultHasher::new()` in `upload_lights` (`scene_buffer/upload.rs`), both from `186234944`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

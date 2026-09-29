# #4988: CONC-D1-2026-09-28-02: Two egui texture hazards rest on the all-slots fence wait but are missing from the rider list

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,sync,renderer,test-gap,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW. This is a test and documentation gap. It is safe today. The precedent is #4851 and #4852, both LOW `sync`/`test-gap`.
- **Dimension**: Vulkan Queue & AS Sync
- **Location**:
  - `crates/renderer/src/vulkan/egui_pass.rs:229-237` (`pending_free` → `free_textures`) and `:250-255` (partial `set_textures` overwrite).
  - The rider list is at `crates/renderer/src/vulkan/sync.rs:45-120`. Its pin is `frames_in_flight_contract_names_every_dependent_resource` at `sync.rs:634`.
- **Status**: NEW. This is the same class as #4601, #4851 and #4852 (all closed), for a resource none of them names.
- **Description**: Two egui operations are safe only because the frame-start wait covers every slot:
  - At the start of frame N's dispatch, `EguiPass::dispatch` frees `pending_free`, the texture IDs from frame N-1's `textures_delta.free`. `Renderer::free_textures` (`egui-ash-renderer mod.rs:415-425`) destroys the image, view and memory **immediately** and frees the descriptor set. Frame N-1's egui draw may have bound and sampled exactly those textures: egui frees a texture after the frame that last paints it. That is safe only if frame N-1 has retired, which only the **all-slots** wait at `sync_and_acquire_frame.rs:81` guarantees. A per-slot wait on `in_flight[N % 2]` retires N-2 only. The code comment at `egui_pass.rs:229-232` ("the fence at the top of `draw_frame` has waited on the previous frame's command buffer") states the premise but is not in the list.
  - The partial `set_textures` overwrite described in finding 01 has no device-side WAR edge, so it rests on the same premise.

  The full-replacement arm (`pos: None` on an existing ID) is self-protected: `from_rgba8`'s `queue_wait_idle` drains frame N-1 before `previous.destroy`.
- **Evidence**: `sync.rs` has 13 listed riders plus the #3429 note on dynamic RGBA. `grep -n "egui\|pending_free" crates/renderer/src/vulkan/sync.rs` finds no match. `the_all_slots_wait_argument_is_pinned` (`sync.rs:750`) and the rider-list guard exist, but neither names egui.
- **Trigger Conditions**: No live hazard. It becomes a use-after-free of an image and descriptor set the moment #4606's throughput work narrows the wait to `&[in_flight[frame]]` without re-deriving the riders.
- **Impact**: None at HEAD. The risk is to future changes: freeing an image or descriptor set that is still in use leads to device loss or a GPU page fault in the debug overlay.
- **Verification Path**: `cargo test -p byroredux-renderer frames_in_flight_contract` after adding the entry; it is a source pin.
- **Related**: #870, #3643, #4601, #4606, #4851, #4852.
- **Suggested Fix**: Add a rider entry to the #870 block for `egui_pass.rs`'s `pending_free` and the partial `set_textures` overwrite, and add `("pending_free", include_str!("egui_pass.rs"))` to the test's resource table. Alternatively, retire egui frees through a `MAX_FRAMES_IN_FLIGHT`-deep per-slot ring, so the wait can be narrowed safely.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D1-2026-09-28-02) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in the other one-time/partial texture paths (texture_registry, dynamic_rgba, overlay uploads)
- [ ] **DROP**: If Vulkan objects change, teardown ordering is still correct (see the three load-bearing orderings in `context/teardown.rs`)
- [ ] **TESTS**: A regression test (or a recorded `BYRO_VALIDATION=1` capture) pins this specific fix

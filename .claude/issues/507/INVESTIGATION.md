# Investigation

- draw.rs:720-721 bound `self.pipeline` (Opaque, non-two-sided).
- draw.rs:787-791 initializes `last_pipeline_key` to sentinel
  `Blended { u8::MAX, u8::MAX, false }`.
- draw.rs:845: every first batch mismatches sentinel → rebinds.
- draw.rs:971-977: UI overlay binds `self.pipeline_ui` regardless.
- Empty-scene frame: no batches, no UI → just `cmd_end_render_pass`.
  LOAD_OP_CLEAR handles clear; no pipeline needed.

Descriptor set binds at :740-760 use `self.pipeline_layout`, not the
pipeline, so they're independent.

## No regression test
draw_frame requires a live Vulkan device. Project has no headless
Vulkan test harness. The assertion of correctness is via code review
+ the fact that ~all scene batches already bind their own pipeline.

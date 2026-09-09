Closing as **already resolved** — not by a fix made for this issue, but by `ea6d2437` (#4007), which landed after this sweep's `229306ce` base.

**The premise no longer holds.** #4007 addressed the same mechanism from the other direction (D13-01, "the rigid-history limb must be order-independent") and added a one-shot latch, `suppress_rigid_history_next_build`, raised by `signal_temporal_discontinuity` alongside the `clear()`:

```rust
self.previous_rigid_models.clear();
self.suppress_rigid_history_next_build = true;
```

`build_and_upload_instances` consumes it with `std::mem::take` and gates the lookup on it:

```rust
if uses_rigid_history && !camera_cut && !suppress_rigid_history { … }
```

That closes all three in-frame call sites this issue names:

- The two `post_passes.rs` sites (`record_taa_pass` #3605, `record_upscale_pass` #2519) run after `build_and_upload_instances`, so the latch survives `draw_frame`'s end-of-frame swap and is consumed by the *next* frame's build — which is the frame the contract is about.
- The `assemble_camera_and_lights.rs` site runs before the build and is consumed the same frame. It is still belt-and-braces next to the `camera_cut` gate, but it is no longer inert.

**The suggested fix would now be wrong.** This issue proposed documenting that "the `previous_rigid_models` clear is only meaningful to callers running outside `draw_frame`". Post-#4007 that statement is false, and the doc comment already says the opposite, correctly:

> The `clear()` drops the data; the latch makes the suppression survive `draw_frame`'s end-of-frame swap, so this holds whether the caller runs before `build_and_upload_instances` or after it (#4007).

Writing the proposed text would have re-introduced the drift this issue was filed to prevent.

**Pinned, not just fixed.** `rigid_history_suppression_tests` in `build_and_upload_instances.rs` guards both halves — that the latch is *consumed* (`mem::take`, not merely read, so it suppresses one build rather than the whole session) and that `signal_temporal_discontinuity` still raises it. The forward-looking risk this issue actually cared about — "a future in-frame caller added on the belief the clear is effective" — is now covered by construction rather than by a comment.

Sibling findings #4033, #4034 and #4035 from this sweep are fixed in the accompanying commit.

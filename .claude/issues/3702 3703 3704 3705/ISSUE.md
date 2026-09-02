# #3702 — ECS-2026-08-30-D10-02 (LATENT): non-playing layer never ticks its blend-out

**Severity**: MEDIUM · **Dimension**: Animation Runtime
**Location**: `crates/core/src/animation/stack.rs` (`advance_stack` ~:167-226, the `if !layer.playing { continue; }` guard at ~:169-171; `cleanup_finished` ~:152-159)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D10-02)

LATENT — no live repro (AnimationStack not registered in production `boot.rs`). `advance_stack`'s `if !layer.playing { continue; }` skips the *whole* per-layer body, including blend timers. `cleanup_finished` only retires layers where `blend_out_total > 0.0 && blend_out_remaining <= 0.0`. A paused layer that `play()` scheduled for fade-out keeps `blend_out_remaining == blend_time` forever, is never retired, and contributes full weight to every blend indefinitely.

**Suggested Fix**: Move the blend-timer advance above the `!layer.playing` guard so fades are wall-clock and independent of clip playback, or have `cleanup_finished` also drop layers with `effective_weight() < 0.001 && !playing`.

---

# #3703 — ECS-2026-08-30-D10-03 (LATENT): AnimationStack path writes absolute position as RootMotionDelta

**Severity**: MEDIUM · **Dimension**: Animation Runtime
**Location**: `byroredux/src/systems/animation.rs` (stack path ~:945-964) vs player path (~:706-713) and `sampled_root_motion_delta` (~:85-107); consumer `byroredux/src/systems/cinematic.rs` (`cinematic_root_motion_system`, ~:119-130)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D10-03)

LATENT — no live repro. `sampled_root_motion_delta` exists to avoid compounding motion; the `AnimationPlayer` path uses it. The `AnimationStack` path instead feeds `split_root_motion(pos).1` (raw absolute-derived delta) into `root_motion`, stored into `RootMotionDelta`. Consumer `cinematic_root_motion_system` integrates this every frame, so an absolute-position payload compounds.

**Suggested Fix**: Route the stack path's accum-root contribution through `sampled_root_motion_delta` using the dominant layer's `(prev_time, local_time)` and clip, mirroring the player path.

---

# #3704 — ECS-2026-08-30-D10-04: text keys dropped when delta > duration but not an exact multiple

**Severity**: MEDIUM · **Dimension**: Animation Runtime
**Location**: `crates/core/src/animation/text_events.rs` (`collect_text_key_events`, the #3034 arm, ~:63-117)
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D10-04)

Coverage gap in #3034's fix, not a regression. The "fire every key once" arm is gated on `curr_time == prev_time` (exact multiple of duration). For `|delta| > duration` with non-zero residual, the playhead traverses whole periods but only the residual window is scanned — keys outside the residual window fire zero times.

**Suggested Fix**: Widen the arm to `applied_delta.abs() >= clip.duration` on `Loop` clips (fire every key once, then still scan the residual window), keeping the `applied_delta != 0.0` guard #3470 added.

---

# #3705 — ECS-2026-08-30-D10-05: clip release driven by NIF-cache LRU with no liveness check

**Severity**: MEDIUM · **Dimension**: Animation Runtime / Component Lifecycles
**Location**: `byroredux/src/cell_loader/nif_import_registry.rs` (`NifImportRegistry::insert` eviction loop, ~:488-530); `crates/core/src/animation/registry.rs` (`AnimationClipRegistry::release`, ~:156-191); callers at `byroredux/src/streaming_helpers.rs:544`, `byroredux/src/cell_loader/references/mod.rs:136`, `byroredux/src/cell_loader/partial.rs:151`
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D10-05)

`NifImportRegistry::insert` evicts by pure LRU, returns the evicted entry's clip handle, and every caller forwards it to `AnimationClipRegistry::release` — nothing consults whether a live `AnimationPlayer` still holds that handle. Post-release the slot reads as an empty clip; the still-loaded REFR's animation silently and permanently stops.

**Suggested Fix**: Refcount clip handles against live `AnimationPlayer`/`AnimationLayer` holders so eviction can only release clips nothing is playing; alternatively store the source path on the player so a released clip can be lazily rebuilt via `get_or_insert_by_path`.

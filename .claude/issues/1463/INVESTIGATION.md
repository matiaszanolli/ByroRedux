# Investigation — #1463 integration param UBO single-buffered

**Domain:** renderer (volumetrics GPU memory)

## Decision
LOW / latent. `integration_param_buffer` is a single `Option<GpuBuffer>` written
once at construction with the immutable `dt = DEFAULT_VOLUME_FAR / FROXEL_DEPTH`.
Sound today (read-only → all FIF integrate sets may alias it). The WAR hazard
only appears if Phase 5 makes `dt` per-frame without first converting to per-FIF.
The issue's recommended immediate action is exactly a constraint comment at the
construction site — done.

## Fix (documentation only — zero behavioural change)
- `volumetrics.rs:403` (construction site): added the #1463 constraint comment
  explaining why single-buffering is sound now and what must change first
  (convert to per-FIF `Vec<GpuBuffer>`, mirror `param_buffers`) before `dt` goes
  dynamic.
- Cross-listed as item 2 of the FLIP CHECKLIST on the `VOLUMETRIC_OUTPUT_CONSUMED`
  const.

## Completeness checks
- [x] **SIBLING**: references the existing per-FIF `param_buffers` as the pattern to mirror.
- [x] **DROP**: noted that a future per-FIF `Vec` teardown must cover every slot
  (parity with the current single-buffer `.take()` at `volumetrics.rs`).
- [x] **TESTS**: N/A — documentation only. cargo test 2790 pass.

## Residual
The per-FIF conversion is Phase 5 work, gated by the FLIP CHECKLIST.

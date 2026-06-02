# #1449 — MEM-01: evict_unused_blas immediate-destroy assumes no in-flight TLAS during multi-batch cell load

_Snapshot as filed (2026-06-02) from AUDIT_RENDERER_2026-06-02.md. GitHub is authoritative for live state._

- **Severity**: LOW (latent; gated behind a future refactor)
- **Dimension**: GPU Memory / Acceleration Structures
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:991` (`evict_unused_blas`), invoked mid-batch `:507-521`; `frame_counter` bump `:397`
- **Status**: NEW (observation; no code change recommended now)

## Description
`evict_unused_blas` destroys the acceleration structure **immediately** (no `pending_destroy_blas` round-trip), safe via the `MIN_IDLE_FRAMES` const-assert. During a multi-batch cell load, `build_blas_batched` bumps `frame_counter` once per batch (`:397`) without `draw_frame`/`build_tlas` between batches, so a BLAS still referenced by the in-flight previous TLAS could read as `idle >= min_idle` and be destroyed.

## Impact
**Not reachable today** — cell loads are gated behind the load flow, not interleaved with live rendering (and `drop_blas` already uses deferred destroy + TLAS full-rebuild). The risk is a theoretical TLAS-referenced-BLAS use-after-free **only** if a future streaming-during-render refactor runs `build_blas_batched` while frames are genuinely in flight.

## Suggested Fix
Add a one-line invariant note on `evict_unused_blas` stating the immediate-destroy path additionally assumes cell-load bursts are not interleaved with in-flight draw frames. If streaming-during-render lands, route eviction through `pending_destroy_blas` (as `drop_blas`/`drop_skinned_blas` already do), or gate the per-batch `frame_counter` bump so it cannot outrun retired frames.

## Completeness Checks
- [ ] **DROP**: if rerouted through pending_destroy, verify countdown + TLAS full-rebuild force are set (parity with drop_blas)
- [ ] Invariant note added at the immediate-destroy site

_Filed from [docs/audits/AUDIT_RENDERER_2026-06-02.md](../blob/main/docs/audits/AUDIT_RENDERER_2026-06-02.md)._

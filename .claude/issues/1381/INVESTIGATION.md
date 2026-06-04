# Investigation — #1381 PERF-D2-NEW-03 dhat alloc-regression coverage

**Domain:** performance / tech-debt (infra) · **Partial: binary feature shipped**

## What shipped
**Binary `dhat-heap` feature** (the issue's primary "wire dhat-heap feature" ask):
- `byroredux/Cargo.toml`: `dhat = { workspace = true, optional = true }` +
  `[features] dhat-heap = ["dep:dhat"]` (no new workspace dep — dhat was already
  a workspace dep behind the NIF crate's gate).
- `byroredux/src/main.rs`: gated `#[global_allocator] DHAT_ALLOC` + a whole-run
  `dhat::Profiler::new_heap()` held for `main`'s lifetime (writes `dhat-heap.json`
  on exit). No-op + zero cost on default builds.
- Verified: `cargo check -p byroredux` (default, dhat absent) and
  `--features dhat-heap` both build; existing NIF heap gate still passes under
  `--features dhat-heap`.

## Deferred sub-items (kept open on #1381)
1. **Cell-load→unload allocation assertion** — needs a live Vulkan device + on-disk
   game data (smoke-test tier); cannot run headless / in CI here.
2. **NIF fixture geometry/particle extension** — `heap_allocation_bounds.rs` covers
   a bare NiNode only. Extending to BSTriShape / NiPSysEmitter requires hand-
   authoring byte-exact multi-block NIFs across version gates: NiAVObject +
   NiObjectNET base + NiGeometry version branches + a separate NiTriShapeData /
   NiPSysData block. There is **no reusable proven synthetic geometry/particle
   builder** in the test suite (all `synthetic_fixtures.rs` builders are single
   NiNode; real-NIF tests need game data). Authoring this blind — validated only
   by "does it parse" — is high-risk/high-effort and disproportionate to a LOW
   test; flagged for a focused dedicated effort with local `--features dhat-heap`
   iteration rather than shipped fragile.

## Decision
Shipped the validated binary feature; left #1381 open scoped to the two deferred
sub-items rather than commit fragile, unvalidatable fixture bytes.

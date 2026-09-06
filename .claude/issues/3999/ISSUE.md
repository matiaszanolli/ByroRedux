# #3999 — REN-2026-09-06-D1-05: four `pub` accessors on `AccelerationManager` have zero call sites, and three of them name a consumer that does not exist — so the deferred-destroy backlog `#3840` introduced is unobservable at runtime

**Labels**: low, memory, renderer, shaders, tech-debt, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-05), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: AS Correctness (observability / dead API)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs`
  (`pending_destroy_blas_count`, `pending_destroy_static_bytes`,
  `pending_destroy_scratch_count`),
  `crates/renderer/src/vulkan/acceleration/memory.rs` (`total_blas_bytes`).
  Ground truth for the named consumers:
  `byroredux/src/commands/mod.rs` (the registry),
  `crates/renderer/src/vulkan/context/mod.rs` (`fill_scratch_telemetry`, what
  `ctx.scratch` actually prints)
- **Status**: NEW (not in the OPEN cache). **Distinct from — and NOT the same
  claim as — the false `REN-2026-08-30-D1-01` / `REN-2026-09-05-D1-02`:
  `integrity_snapshot()` *does* have a live consumer** (see
  `REN-2026-09-06-D1-02`). These four do not.
- **Description**: A `grep` for each accessor across `crates`, `byroredux` and
  `tools` returns only its own definition:
  | Accessor | Docstring claims | Reality |
  |---|---|---|
  | `total_blas_bytes()` | *"reported by `total_blas_bytes()` for telemetry / *tex.stats* console output"* | No caller. There is **no *tex.stats* command** in the registry. Total BLAS VRAM is not surfaced anywhere. |
  | `pending_destroy_blas_count()` | *"Surfaced for `drain_pending_destroys`'s unit test and shutdown telemetry — the count must reach zero after a drain. See #732."* | No caller, and no test names it. |
  | `pending_destroy_scratch_count()` | *"Surfaced for the deferred-destroy regression test and shutdown telemetry … See #1782."* | No caller, and no test names it. |
  | `pending_destroy_static_bytes()` | *"Companion to `pending_destroy_blas_count` for `ctx.scratch` telemetry — the count alone can't show how much VRAM the queue is holding. See #3840."* | No caller. `ctx.scratch` exists, but `fill_scratch_telemetry` emits host-side `Vec`/`HashMap` `(len, capacity)` rows only — no BLAS byte counters. |

  The last row is the sharp one: `fa5c4191` added the accessor **yesterday**
  specifically so an operator could see how much VRAM the deferred-destroy queue
  is holding, and then did not connect it. `live_static_blas_count()` /
  `live_skinned_blas_count()` are the counter-example that shows the wiring
  pattern is available and cheap — they *do* have a consumer
  (`byroredux/src/ownership_sample.rs`).
- **Evidence**:
  - `grep -rn "\btotal_blas_bytes\b" --include='*.rs' crates byroredux tools` →
    definition, field, and doc/comment mentions only; no `total_blas_bytes()`
    call expression anywhere.
  - Same for `pending_destroy_blas_count`, `pending_destroy_scratch_count`,
    `pending_destroy_static_bytes` (the last has field-level reads inside
    `blas_static.rs` itself, but zero calls to the `pub` accessor).
  - `grep -rn '"tex\.stats"' byroredux/src/commands/*.rs` → no match; the
    registered commands in that family are `ctx.scratch` and the memory-frag
    command `mem` + `.frag` (spelled out to avoid reading as a shader path).
  - `crates/renderer/src/vulkan/context/mod.rs`'s `fill_scratch_telemetry` pushes
    `ScratchRow { name, len, capacity, elem_size_bytes }` for
    `gpu_instances_scratch`, `frame_lights_scratch`, `previous_models_scratch`,
    `batches_scratch`, the two rigid-motion maps, `indirect_draws_scratch`,
    `terrain_tile_scratch`, the skin-path sets, and (per #3693) the three
    `tlas_*_scratch` Vecs — all host-side capacities, no device bytes.
- **Impact**: BLAS device residency and the deferred-destroy backlog cannot be
  read from a running engine, so the exact failure `REN-2026-09-06-D1-01`
  describes (a batch overshooting the real budget by the queued amount) is not
  diagnosable in the field even after `#3840` computed the number. Secondary:
  four `pub` items with docstrings that assert consumers which do not exist —
  the same "documented surface that isn't there" pattern that produced the
  *mem.stats* / *tex.stats* family of `REN-LOW L-1` / `L-6` findings.
- **Related**: `#3840` (added the newest of the four), `#732` / `#1782` (the two
  older ones), `REN-LOW L-1` / `L-6` (the *mem.stats* precedent), and
  `REN-2026-09-06-D1-01` (the accounting gap these would have made visible).
- **Suggested Fix**: Add four rows to `fill_rt_integrity_stats`'s neighbour
  (`ScratchTelemetry` is host-only; a `mem.frag`- or `ctx.scratch`-adjacent
  device-side block, or extra `RtIntegrityStats` fields, all work) surfacing
  `total_blas_bytes`, `static_blas_bytes`, `pending_destroy_static_bytes` and
  the two pending counts — and correct the three docstrings so no accessor names
  a command that does not exist. If a consumer is genuinely not wanted for the
  count accessors, delete them rather than leave `pub` items whose docs describe
  a test that was never written.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix

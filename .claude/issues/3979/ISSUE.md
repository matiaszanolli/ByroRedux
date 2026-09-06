# #3979 — REN-2026-09-06-D1-01: `#3840`'s `resident_static_blas_bytes` is wired into the one predicate where it cannot change any outcome — the "admission" its own docstring describes does not exist, so a BLAS batch still allocates against headroom the GPU has not released

**Labels**: medium, memory, renderer, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs`
  (`resident_static_blas_bytes`, and its single consumer inside
  `build_blas_batched`'s mid-batch check);
  `crates/renderer/src/vulkan/acceleration/predicates.rs`
  (`should_evict_mid_batch`, `blas_over_budget`)
- **Status**: NEW (fresh churn — `fa5c4191`, 2026-09-05; not in the 151-issue
  OPEN cache; `#3840` is the *fix* commit, not an open issue)
- **Description**: `fa5c4191` added `pending_destroy_static_bytes` and the
  derived `resident_static_blas_bytes()` to fix a real accounting gap — eviction
  credits `static_blas_bytes` the instant it queues an entry, but the allocator
  free happens `DEFAULT_COUNTDOWN` frames later inside `draw_frame`, which never
  runs during a streaming batch. The counter itself is correct and complete
  (verified: every static push credits it, both destroy paths release it, skinned
  entries are excluded via `counted_in_static_bytes`). The **wiring** is the
  problem. `resident_static_blas_bytes()` has exactly one non-test call site in
  the workspace — the first argument of `should_evict_mid_batch` — and at that
  site it is provably inert:
  - trigger: `(A + pending) * 10 >= budget * 9` (the 90 % line);
  - callee gate and loop break: `blas_over_budget(static_blas_bytes, pending, budget)`
    = `paper + pending > budget` (the 100 % line), deliberately on the **paper**
    figure.

  Because `budget > 0.9 · budget`, every state in which the callee can actually
  evict already satisfies the trigger *with the paper figure*. Substituting
  `resident` (= `paper + queued`) can only widen the trigger set into states
  where the callee immediately early-returns. So the change cannot cause one
  extra byte to be evicted, and — since nothing in `build_blas_batched` ever
  declines to allocate — it cannot prevent one extra byte from being allocated
  either. Its only effect is additional no-op calls to `evict_unused_blas` every
  `BATCH_EVICTION_CHECK_INTERVAL` iterations.

  Meanwhile `resident_static_blas_bytes`'s docstring asserts the opposite:
  *"This is the figure admission checks must use … letting a batch allocate
  against headroom that does not exist yet (#3840)."* There is no admission
  check. The hazard the docstring names is still open.
- **Evidence**:
  - `grep -rn "resident_static_blas_bytes" --include='*.rs' crates byroredux` →
    5 hits: the definition, one doc cross-reference in
    `acceleration/mod.rs`, one comment inside `evict_unused_blas`, the single
    call inside `build_blas_batched`'s mid-batch check, and the source-shape
    assertion in `acceleration/tests/blas_static_tests.rs`. **Zero** other
    consumers.
  - `evict_unused_blas`'s first statement after the `let _ = (device, allocator);`
    is `if !blas_over_budget(self.static_blas_bytes, pending_bytes, self.blas_budget_bytes) { return; }`
    — the paper figure, by design (`#3840` comment at the push site, pinned by
    `mid_batch_trigger_uses_resident_bytes_but_the_evict_loop_does_not`).
  - `build_blas_batched`'s Phase-1 loop calls
    `GpuBuffer::create_device_local_uninit(...)?` unconditionally on every
    iteration; the only budget interaction in the loop is the
    `should_evict_mid_batch` → `evict_unused_blas` pair. The compaction-phase
    call (`alloc_compact`, `pending = total_before + total_after`) is likewise
    only an eviction request, not an admission gate.
  - The reasoning is sound *within* the current design: the `#3840` comment is
    right that a resident-based **loop break** would never be satisfied (each
    iteration just moves bytes from `static_blas_bytes` into
    `pending_destroy_static_bytes`). Eviction genuinely cannot reclaim inside a
    batch. That is precisely why the missing piece has to be admission, not
    eviction.
- **Impact**: On the 6 GB RT-minimum target the doc calls out
  (`blas_static.rs`'s own *"this is a 6 GB-RT-minimum-target path"* note), a
  cell-load batch that evicts early and then keeps allocating can overshoot the
  real static-BLAS residency by up to the whole deferred-destroy backlog before
  anything pushes back. The blast radius is bounded, not catastrophic:
  `build_blas_batched` handles allocator failure with full rollback of
  `prepared` + `compact_accels` + the query pool (#1097 / #2926) and returns
  `Err`, which `restore_missing_static_blas_for_draws` / the cell loader turn
  into a `warn!` and a cell whose rigid geometry is missing from RT for that
  frame. So the failure mode is "RT loses the tail of a cell under VRAM
  pressure", not corruption or device loss. On the 12 GB dev card it is
  unreachable. Secondary impact: the docstring is a false statement about a
  memory-safety-adjacent invariant, and it is the kind a future reader will
  trust rather than re-derive.
- **Related**: `#3840` (the commit), `#1792` / `PERF-D3-NEW-01` (the previous
  round of the same "the gate and the trigger disagree" bug in this exact pair
  of predicates), `#1449` / `#1782` (why the deferral itself must stay),
  `#3540` / `plan_static_blas_restore` (the precedent policy: *decline the pass*
  rather than thrash).
- **Suggested Fix**: Give the counter an actual admission consumer. The
  cheapest correct shape mirrors `plan_static_blas_restore`: inside
  `build_blas_batched`'s Phase-1 loop, once
  `resident_static_blas_bytes() + pending_bytes` exceeds `blas_budget_bytes`
  **and** the preceding `evict_unused_blas` reclaimed nothing, stop admitting
  further meshes, finish the batch with what is already prepared, and return the
  partial count with a one-shot `warn!` — the same "decline rather than
  converge-never" policy #3540 installed for the recovery pass. Do **not**
  attempt to fix this by ticking `pending_destroy_blas` at a batch boundary:
  that is exactly the #1449 / #1782 use-after-free class, and the countdown's
  whole purpose is to stand in for a fence wait `build_blas_batched` does not
  have. This is CPU-side bookkeeping only — no barrier, render-pass or pipeline
  change is involved.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

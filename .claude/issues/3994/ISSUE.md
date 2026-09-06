# #3994 — REN-2026-09-06-D5-03: #3840's `resident_static_blas_bytes()` cannot change any behaviour — its only consumer is a trigger whose callee re-tests with the paper figure and returns

**Labels**: medium, memory, renderer, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D5-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM (defence-in-depth gap: the machinery is carried, the
  hazard it names is not addressed)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs` —
  `AccelerationManager::resident_static_blas_bytes`,
  `pending_destroy_static_bytes`, and the mid-batch trigger inside
  `build_blas_batched`; `AccelerationManager::evict_unused_blas` (same file);
  predicates `should_evict_mid_batch` and `blas_over_budget`
  (`acceleration/predicates.rs`); `BlasEntry.counted_in_static_bytes`
  (`acceleration/types.rs`). Guard: the
  `pending_destroy_static_bytes_stays_balanced_tests` module
  (`acceleration/tests/blas_static_tests.rs`).
- **Status**: **NEW.** Landed yesterday in `fa5c4191`. No matching open issue
  (searched `3840`, `resident`, `pending_destroy`, `eviction`, `budget`).
- **Description**: #3840's stated hazard, in
  `resident_static_blas_bytes`'s own doc comment, is that
  `static_blas_bytes` is "the *paper* figure — eviction credits it the moment
  an entry is queued … letting a batch **allocate against headroom that does
  not exist yet**", and that the resident figure "is the figure admission
  checks must use".

  There is exactly one production call site
  (`grep -rn "resident_static_blas_bytes"` → the definition, one call, one
  doc reference, one source-scan assertion). It is the 90 % **trigger**:

  `should_evict_mid_batch(self.resident_static_blas_bytes(), pending_bytes, budget)`
  → `(static + pending_destroy + pending) * 10 >= budget * 9`

  and the trigger's only action is `self.evict_unused_blas(device, allocator,
  pending_bytes)`, whose first statement is the 100 % **reclaim gate** on the
  *paper* figure:

  `if !blas_over_budget(self.static_blas_bytes, pending_bytes, budget) { return; }`
  → `(static + pending) > budget`

  Because `resident >= paper` always, the resident trigger is a strict
  superset of the paper trigger. In the band it newly covers
  (`static + pending <= budget < 0.9⁻¹ · (static + pending_destroy + pending)`)
  the callee's gate rejects and returns immediately. Outside that band both
  spellings agree. So swapping in the resident figure adds trigger firings
  that do nothing and changes no eviction decision — the batch continues
  allocating against the same headroom #3840 says does not exist.

  This is the shape the sibling comment fifteen lines below already warns
  about, for `pending_bytes`, in the #1792 write-up: *"the trigger above
  fired, but the callee it called was structurally blind to the very bytes
  that triggered it."*

  Note the paper-figure choice inside `evict_unused_blas` is **correct and
  deliberate** — its own #3840 comment explains that a resident-based loop
  condition would never improve, because each eviction moves the same bytes
  from `static_blas_bytes` into `pending_destroy_static_bytes`, so the loop
  would evict every candidate. The gap is that nothing else consumes the
  resident figure: there is no admission check anywhere that can stop or
  throttle a batch, and `tick_deferred_destroy` (the only thing that actually
  reduces residency) cannot run mid-batch — its sole caller is inside
  `draw_frame`.
- **Evidence**:
  - `predicates.rs`: `should_evict_mid_batch(total_live, pending, budget)` =
    `projected.saturating_mul(10) >= budget_bytes.saturating_mul(9)`;
    `blas_over_budget(static, pending, budget)` =
    `static_blas_bytes.saturating_add(pending_bytes) > budget_bytes`.
  - `blas_static.rs`: the only `resident_static_blas_bytes()` call is the
    first argument of the `should_evict_mid_batch` in `build_blas_batched`.
  - `evict_unused_blas`'s early-return and its per-candidate `break` both use
    `self.static_blas_bytes`.
  - The pin is a **source-scan**:
    `BLAS_STATIC_RS.contains("self.resident_static_blas_bytes(),")` — it
    asserts the call is textually present, so it passes whether or not the
    call can affect anything.
- **Impact**: No correctness harm and no leak — the bookkeeping itself is
  balanced and correct (verified: `drop_blas` and `evict_unused_blas` both
  credit `pending_destroy_static_bytes`; `tick_deferred_destroy` debits
  exactly the entries whose `counted_in_static_bytes` is set;
  `drain_pending_destroys` zeroes it; `blas_skinned.rs` pushes
  `counted_in_static_bytes: false` so skinned entries never touch the static
  counter). The cost is that a closed issue's hazard is still live: on a
  cell transition, the outgoing cell's BLAS bytes sit in
  `pending_destroy_static_bytes` until the next `draw_frame`, while
  `build_blas_batched` for the incoming cell allocates as if they were
  already freed. On the 12 GB dev card this is unreachable; on a 6 GB card at
  a large streaming boundary it is the allocator-failure path #3840 named.
  It also means three new struct fields, an accessor, and a guard test are
  carried for no behavioural effect, which is the kind of thing that reads as
  "already handled" to the next reader.
- **Related**: #3840 (the issue this closed), #1792 (the identical
  trigger-fires-callee-blind shape, for `pending_bytes`), #1449 (deferred
  eviction, which created the paper-vs-resident divergence),
  `REN-2026-09-05-D1-02` (a sibling "computed and thrown away" observation in
  the same subsystem, still unfiled).
- **Suggested Fix**: Decide which of the two this is meant to be and make the
  code say so. Either (a) give the resident figure a consumer that can act —
  the natural one is an admission check in `build_blas_batched`'s Phase 1
  loop that stops adding meshes to *this* batch when
  `resident + pending + next_size > budget`, deferring the remainder to the
  next batch after a tick has run; or (b) if throttling is not wanted, revert
  the trigger to `self.static_blas_bytes` and keep
  `resident_static_blas_bytes` purely as telemetry, documented as such. Either
  way, replace the source-scan pin with a behavioural one: a pure-function
  test over `should_evict_mid_batch` + `blas_over_budget` showing that the
  band where they disagree produces a different outcome. Today no test can
  fail if the resident call is deleted outright.

---

### LOW

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

# #4001 — REN-2026-09-06-D1-07: `drop_skinned_blas` is the one deferred-destroy push site that bypasses `DEFAULT_COUNTDOWN`

**Labels**: low, renderer, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-07), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: AS Correctness (code quality)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`
  (`drop_skinned_blas`); contract in `crates/renderer/src/deferred_destroy.rs`
  (`DEFAULT_COUNTDOWN`, `DeferredDestroyQueue::push`)
- **Status**: NEW (not in the OPEN cache)
- **Description**: `DeferredDestroyQueue::push`'s doc says *"Production callers
  pass [`DEFAULT_COUNTDOWN`] so the item survives at least
  `MAX_FRAMES_IN_FLIGHT` frames"*, and `DEFAULT_COUNTDOWN` exists precisely so a
  future `MAX_FRAMES_IN_FLIGHT` bump propagates in one place. Three of the four
  push sites into `pending_destroy_blas` / `pending_destroy_scratch` pass
  `DEFAULT_COUNTDOWN`; `drop_skinned_blas` passes `MAX_FRAMES_IN_FLIGHT as u32`
  directly.
- **Evidence**: `blas_skinned.rs`:
  `self.pending_destroy_blas.push(entry, MAX_FRAMES_IN_FLIGHT as u32);` against
  `blas_static.rs`'s `self.pending_destroy_blas.push(entry, DEFAULT_COUNTDOWN);`
  (twice) and `self.pending_destroy_scratch.push(old, DEFAULT_COUNTDOWN);`.
  `crates/renderer/src/deferred_destroy.rs`:
  `pub(crate) const DEFAULT_COUNTDOWN: u32 = crate::vulkan::sync::MAX_FRAMES_IN_FLIGHT as u32;`
- **Impact**: **None behaviourally, today or after any `MAX_FRAMES_IN_FLIGHT`
  bump** — `DEFAULT_COUNTDOWN` *is* `MAX_FRAMES_IN_FLIGHT as u32`, so the two
  expressions are identical by construction and cannot diverge. Reported only
  because the module doc states a convention this site does not follow, and
  because a reader auditing the deferred-destroy contract has to re-derive the
  equivalence at this one site. Filed as the lowest-priority item in this run;
  drop it if the fix budget is tight.
- **Related**: `#372`, `#1449`, `#1782`, `#2481`.
- **Suggested Fix**: One-line substitution to `DEFAULT_COUNTDOWN` (already
  imported in the sibling module; add the `use` in `blas_skinned.rs`).

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

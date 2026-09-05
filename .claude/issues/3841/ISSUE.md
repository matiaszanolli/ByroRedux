# #3841: PERF-D3-2026-09-05-03: `shrink_tlas_to_fit` still carries the pre-#2929 "the slot is destroyed outright" prose, contradicted by its own body 25 lines later

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D3-2026-09-05-03) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3841 --json state`.

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-05.md` (PERF-D3-2026-09-05-03), published from `/audit-suite volumetrics-deep`. Premise re-verified against HEAD at publish time.

> Note: `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: GPU Memory Pressure
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs:155-158`, `:321-323`
- **Status**: NEW (re-confirmation of prior-audit item `PERF-D3-2026-08-30-04`; still open, no tracking issue found)
- **Description**: #2929 changed `shrink_tlas_to_fit` from destroy-now to
  record-intent (`tlas_shrink_pending[slot_index] = true`, actually performed
  later by `ensure_tlas_state`'s allocate-then-swap). Two comments still
  describe the old behaviour, one inside the *same doc block* as the
  correction:
  - `memory.rs:155-158`: "The slot is destroyed outright; the next
    [`Self::build_tlas`] call sees `tlas[slot_index].is_none()`..." —
    contradicted by lines 180-185 and 205-231 of the same function.
  - `memory.rs:321-323`: "Slot was destroyed (e.g. by `shrink_tlas_to_fit` on
    the previous tick)" — explicitly corrected by `shrink_tlas_scratch_to_fit`'s
    own doc at lines 262-266 ("**Not** produced by `shrink_tlas_to_fit` since
    #2929").
  Verified against live code: the function returns `true` after only setting
  the pending flag and logging; it never `take()`s the slot. Reserve floors
  are intact — `WORKING_SET_FLOOR` (8192) clamps the shrink target at
  `memory.rs:199`, `MIN_TLAS_INSTANCE_RESERVE` still pads the build path.
- **Evidence**: `memory.rs:232-242` — `let old_max = slot.max_instances;
  self.tlas_shrink_pending[slot_index] = true;` then `true`. No `take()`, no
  destroy.
- **Impact**: Documentation only, but on a `# Safety`-adjacent doc block
  governing TLAS lifetime (an `unsafe fn`). A maintainer trusting the stale
  text could conclude `tlas[slot]` can be `None` after a shrink and
  reintroduce the exact dangling-descriptor hazard #2929 removed (scene
  set-1 binding 2 naming a destroyed `VkAccelerationStructureKHR`, not
  `PARTIALLY_BOUND`, statically used by `triangle.frag`).
- **Related**: #2929 / CON-D1-01; #2915 / REN-D1-03; prior-audit
  `PERF-D3-2026-08-30-04`.
- **Suggested Fix**: Delete the two stale sentences; the #2929 block at
  `memory.rs:205-231` already states the real contract.
- **Confidence**: High.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

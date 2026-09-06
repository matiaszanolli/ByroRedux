# #4033 — REN-2026-09-06-D4-03: the authoritative submission-order block enumerates two barrier-only steps but omits the frame's two most load-bearing barriers

**Labels**: low, renderer, shaders, sync, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D4-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `docs/engine/shader-pipeline.md` (§"Per-Frame Submission Order",
  the fenced 24-step block — steps 8 and 9 are the only `[Barrier]` rows)
- **Status**: NEW — doc gap, code right
- **Description**: The block deliberately gives barrier-only work its own
  numbered rows (step 8 *"`SHADER_READ_ONLY_OPTIMAL` on all G-buffer
  attachments"*, step 9 *"caustic accum atomic-add → `SHADER_READ`"*), which
  establishes that barriers are in this doc's scope. Two barriers with far more
  weight than either of those appear nowhere in the document:

  1. **The bulk host-visibility barrier** — `memory_barrier(HOST/HOST_WRITE →
     VERTEX_SHADER | FRAGMENT_SHADER | COMPUTE_SHADER | DRAW_INDIRECT /
     SHADER_READ | SHADER_WRITE | UNIFORM_READ | INDIRECT_COMMAND_READ)` at the
     tail of `build_and_upload_instances`. It is the single publication point
     for the instance SSBO **and** the composite, SVGF, TAA and water parameter
     UBOs, which were deliberately folded onto it across #909, #961 and #1397
     precisely so those passes need no per-dispatch HOST barrier. A reader
     reasoning about why `record_composite_pass` (step 17) has no HOST barrier
     of its own cannot learn it from this doc.
  2. **The frame's only `AS_WRITE → AS_READ` barrier** —
     `ACCELERATION_STRUCTURE_BUILD_KHR`/`ACCELERATION_STRUCTURE_WRITE_KHR` →
     `FRAGMENT_SHADER | COMPUTE_SHADER`/`ACCELERATION_STRUCTURE_READ_KHR` in
     `dispatch_skin_and_cluster`, which publishes both the TLAS build and every
     skinned-BLAS refit and is emitted on both the success and the failure arm
     (#2931). `/audit-severity` puts *"Missing AS barrier (build → shader
     read)"* at a HIGH floor, making it the one frame-graph edge a doc most
     needs to describe. `docs/engine/renderer.md` describes it correctly (after
     `4ea40bd7`, "Fix doc rot: … nonexistent HOST->AS_BUILD barrier", closed
     *REN-2026-08-30-D4-06*); `shader-pipeline.md`
     — the doc the skill designates authoritative for submission order — does
     not mention it at all.

  Also absent, and cheaper to add: `flush_pending_morph_weights` (a host write
  to a mapped buffer that `sync.rs`'s #870 block names as item 5 on the
  both-slots-wait dependency list) and the
  `FRAGMENT_SHADER/SHADER_WRITE → HOST/HOST_READ` selected-ray-probe publish
  barrier between steps 6 and 7.
- **Evidence**:
  - `grep -n "HOST_WRITE\|ACCELERATION_STRUCTURE\|AS_BUILD" docs/engine/shader-pipeline.md`
    → only the descriptor-binding tables (lines with `ACCELERATION_STRUCTURE` as
    a *descriptor type*), nothing in the order block.
  - The barriers themselves: `build_and_upload_instances` (its comment block
    records the #909 / #961 / #1397 fold history) and
    `dispatch_skin_and_cluster` (its comment records #415 / #2931).
  - The probe publish barrier is already source-pinned by
    `selected_ray_probe_is_bounded_and_captures_the_detailed_shadow_query`
    (`scene_buffer/shader_contract_tests.rs`), which asserts its four masks
    against `draw.rs` — so the code side is guarded; only the doc is silent.
- **Impact**: An auditor or maintainer told to trust this block for ordering
  gets a list that names two minor barriers and omits the two that hold the
  frame together. This is the mechanism that produced *REN-2026-08-30-D4-06*
  (a sibling doc describing the AS barrier with the wrong source stage) in the
  first place.
- **Related**: `4ea40bd7` / *REN-2026-08-30-D4-06* (`renderer.md`'s version of
  the same edge), #3830 (another `shader-pipeline.md` accuracy gap, open),
  #3447, #909, #961, #1397, #2931.
- **Needs RenderDoc**: no.
- **Suggested Fix**: Add two rows to the fenced block — one before step 6 for
  the bulk `HOST_WRITE` fold (naming its four dst stages and the four UBOs
  folded onto it) and one inside step 4 for the `AS_WRITE → AS_READ` publish
  (naming that it covers the skinned refits as well as the TLAS, and that it
  runs on both arms). No code change.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

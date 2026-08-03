# SAFE-2026-08-03-04: SKILL doc-rot — audit-safety.md's Dimension 3 leak-inventory descriptions no longer match the code

Severity: low
Source audit: docs/audits/AUDIT_SAFETY_2026-08-03.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2274

**Dimension**: 3 (Memory & Resource Leaks) / meta doc-rot
**Source**: `docs/audits/AUDIT_SAFETY_2026-08-03.md` (SAFE-2026-08-03-04)
**Status**: NEW
**Location**: `.claude/commands/audit-safety/SKILL.md` Dimension 3 bullets (report
cited the pre-split flat-file path `.claude/commands/audit-safety.md`; the skill
now lives as a directory, `SKILL.md` inside it — re-mapped per the repo's
known-splits convention, not a stale/dead reference)

## Description
Two specific claims in the skill's Dimension-3 text are stale relative to the
current source, though the underlying code is correct:

1. **"CPU-side unbounded growth… The MaterialTable dedup map… [is a] known
   per-cell-growth risk."** In fact `MaterialTable::clear()` is called once per
   frame at the top of `build_render_data` (`byroredux/src/render/mod.rs:531`),
   so the dedup map is rebuilt fresh every frame — it cannot grow across cells or
   across the session at all. This is not a risk; it was mis-scoped in the skill
   text.
2. **"the `DeferredDestroyQueue<T>` shared by mesh + BLAS + BLAS-scratch buffer +
   texture + skin compute"** — a grep for `DeferredDestroyQueue<` across
   `crates/renderer/src/` finds exactly three production instantiations:
   `crates/renderer/src/mesh.rs:169` (`(Option<GpuBuffer>, Option<GpuBuffer>)`,
   i.e. mesh vertex/index buffers) and two in
   `crates/renderer/src/vulkan/acceleration/mod.rs:158,175` (`BlasEntry` and the
   BLAS scratch `GpuBuffer`). No instantiation was found for texture or
   skin-compute resources — those subsystems may use a different (and
   unverified-by-this-audit) deferred-free mechanism, or the skill's claim is
   simply inaccurate.

## Evidence
```
byroredux/src/render/mod.rs:518-531        material_table.clear() called at top of build_render_data, every frame
crates/renderer/src/deferred_destroy.rs:42  DeferredDestroyQueue<T> struct definition
crates/renderer/src/mesh.rs:169             deferred_destroy: DeferredDestroyQueue<(Option<GpuBuffer>, Option<GpuBuffer>)>
crates/renderer/src/vulkan/acceleration/mod.rs:158  pending_destroy_blas: DeferredDestroyQueue<BlasEntry>
crates/renderer/src/vulkan/acceleration/mod.rs:175  pending_destroy_scratch: DeferredDestroyQueue<GpuBuffer>
```
No `DeferredDestroyQueue<` instantiation exists for texture or skin-compute
resources anywhere in `crates/renderer/src/`.

## Impact
None on running code — both the mesh and BLAS/scratch drain paths were
independently re-verified correct this pass (tick runs after fence wait in
`context/draw.rs:1369-1401`; shutdown drain in `acceleration/blas_static.rs:100-141`
and `mesh.rs:1391`, both gated on a preceding `device_wait_idle` per their
`# Safety` docs). Only the *skill's own description* is wrong, which risks a
future audit chasing a non-existent "MaterialTable leak" or over-trusting an
unverified texture/skin-compute deferred-destroy claim that may not exist at all.

## Suggested Fix
Update `.claude/commands/audit-safety/SKILL.md` Dimension 3 to (a) drop the
MaterialTable growth-risk framing (it's cleared every frame, not a growth risk),
and (b) either verify and name the actual texture/skin-compute deferred-free
mechanism (if one exists) or narrow the claim to the three confirmed
`DeferredDestroyQueue<T>` users (mesh, BLAS, BLAS-scratch).

## Related
None — no open issue overlaps this finding (checked against 47 open issues,
`/tmp/audit/issues.json`). Same class as closed #2132/#2133 (SKILL-doc-drift from
the prior `AUDIT_SAFETY_2026-07-25` pass).

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix, no code path affected

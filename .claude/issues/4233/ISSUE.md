# FNV-D7-2026-09-11-01: RagdollTemplate attach comment's 'only skeletons do' claim is falsified by 150/220 non-skeleton FNV NIFs

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4233
**Labels**: documentation, low, legacy-compat, game:fnv, physics, doc-rot
**Source**: `/audit-fnv` — `docs/audits/AUDIT_FNV_2026-09-11.md`, finding FNV-D7-2026-09-11-01

**Severity**: LOW
**Dimension**: PHYSAL Ragdoll (FNV Reference Slice) — `/audit-fnv` Dimension 7
**Location**: `byroredux/src/scene/nif_loader.rs:537-556` (attach-site comment)

**Description**: The `RagdollTemplate` attach-site comment in `nif_loader.rs` asserts a self-gating invariant ("only skeletons do" carry a Havok ragdoll articulation; "facegen/armor/clutter loads have `ragdoll: None`") that a live corpus sweep falsifies: 220 FNV NIFs return `Some(ImportedRagdoll)`, of which **150 have no `skeleton` in their path** — armor gore variants, clutter (chandeliers, swinging traps), and one extreme case (`dlc04brainpoddeath01.nif`: 6 authored edges → 104 disconnected components, 110 bodies).

**Evidence**: Corpus-wide measurement across FNV mesh archives: 220 NIFs return `Some(ImportedRagdoll)`; 150 of those have no `skeleton` in their path. The comment's premise ("only skeletons do") is contradicted by this measured population.

**Impact**: No spawn-time asset currently trips this (cell clutter spawns through a code path that never calls `template_from_imported`), so the practical blast radius is the loose-NIF / debug-load / NPC-spawn paths — but the comment gives the next reader a wrong premise, and a solver-side consequence (a `build_ragdoll` forest-diagnostic mis-attributing authored multi-prop fan-out to an upstream constraint drop) is a real risk of the stale comment, even though it's out-of-scope-for-this-dimension to fix directly.

**Related**: `/audit-physics` (owns `build_ragdoll` solver internals).

**Suggested Fix**: Rewrite the comment to state the real structural gate (≥1 constraint block, ≥2 bone-hosted bodies, ≥1 decoded joint) and name the FNV non-skeleton content that legitimately satisfies it; gate explicitly at the call site if skeleton-only really is the intent.

## Completeness Checks
- [ ] **SIBLING**: Check for the same "only skeletons" claim elsewhere (e.g. `docs/engine/physal.md`, other ragdoll-adjacent comments).
- [ ] **TESTS**: N/A — documentation-only fix; no regression test applicable.

# Issue #986

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/986
**Title**: NIF-D5-ORPHAN-B: Band B orphan-parse bucket — FaceGen camera / vertibird treads / behavior graph / Skyrim large-ref / SSE bound sphere (deferred consumers)
**Labels**: enhancement, nif-parser, import-pipeline, medium
**Parent**: #974 (orphan-parse meta) / #869 (original instance)
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: #974 Band B — orphan-parse follow-up (deferred bucket)
**Severity**: MEDIUM (visible but contextual — none fire on the common cell-render path)
**Domain**: NIF import + various ECS consumers

## Description

Five orphan-parse types where the parser dispatched (often via #720 / #728 / #941 / #942 / #720 etc.) but no importer consumer wires the payload. Unlike Band A, these aren't visible on every cell render — they need a specific gameplay event or rendering subsystem to fire.

| Type | Authored on | Visible drop | Closure landed |
|---|---|---|---|
| `BsEyeCenterExtraData` | FaceGen NPC head NIFs | Dialogue camera frames the NIF origin instead of the eye centroid — affects every "talk to NPC" framing | #720 (parser) |
| `BsTreadTransfInterpolator` | Vertibird / Liberty Prime / Power-Armor wheel NIFs | Tread / wheel mesh rolls statically while base moves; magazine eject anim drops | #941 (parser) |
| `BsBehaviorGraphExtraData` | Skyrim+ FaceGen + creature skeleton | Skyrim animation-graph filename never resolved → no behavior-graph-driven animation (still uses KF fallback) | (dispatched, never closed by a specific issue) |
| `BsDistantObjectLargeRefExtraData` | SSE large-reference flag | Distant-LOD large-ref optimization not applied → BLAS for distant statics is full-detail instead of large-ref | #942 (parser) |
| `BsBound` | various | Mesh-precomputed AABB sphere; engine recomputes bounds (wasted CPU per mesh load) | (dispatched, never wired) |

## Why deferred from Band A

Each requires a downstream subsystem that isn't on the critical path:

- **FaceGen eye centroid**: needs the dialogue-framing camera (Stage 2+ of UI / cinematic camera work)
- **Vertibird treads**: needs the `BsTreadTransfInterpolator` animation sampler in `anim.rs` (currently only Comp variants are sampled; orphan-sibling to #978's parse-dispatch fix)
- **Behavior graph**: needs HKX behavior-graph parser (Havok proprietary; tracked as a separate format dependency)
- **SSE large-ref**: needs the distant-LOD batching pass to flip BLAS detail levels (#915 mirrored `evict_unused_blas` is the nearest piece)
- **BsBound**: optimization, not a correctness bug — saves CPU per mesh load but the recomputed bound is correct

## Suggested approach

Land in any order as the supporting subsystems come online:

1. **BsTreadTransfInterpolator** — easiest; uncompressed-channel sampler in `anim.rs` (synergy with #974 / #978)
2. **BsBound** — optimization; replace `compute_aabb_from_vertices` with the authored sphere when present
3. **BsEyeCenterExtraData** — wait for dialogue camera; populate a `FaceGenEyeCenter` component on NPC entities for the camera to read
4. **BsDistantObjectLargeRefExtraData** — wait for distant-LOD batching pass
5. **BsBehaviorGraphExtraData** — capture the filename into `AnimationGraphRef`, mark behavior-graph parser dependency

## Completeness Checks (per-type when landed)

Same as Band A:
- [ ] **SIBLING** when applicable (e.g. wire `BsEyeCenterExtraData` alongside any other FaceGen consumer)
- [ ] **TESTS** — at minimum a downcast assertion for each type added to integration test
- [ ] **DOC** — comment-link the closure issue + parser site so future audits don't re-discover
- [ ] **ECS** — verify any new component is observable from its downstream consumer

## Source quotes (audit report)

> FaceGen dialogue camera frames the NIF origin instead of the eye centroid (#720 parser landed, no consumer).
> Vertibird / Liberty Prime tread animation is static (#941 parser landed, no consumer wired).
> Cloth, decal-placement, large-ref tagging — deferred to specific milestones.

`docs/audits/AUDIT_NIF_2026-05-12.md` § HIGH → NIF-D5-NEW-01 (orphan-parse meta).

Related: #974 (meta), #720, #728, #941, #942, #869.


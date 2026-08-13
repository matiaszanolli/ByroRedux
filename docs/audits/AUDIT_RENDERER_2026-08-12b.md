# Renderer Audit — 2026-08-12b (full 23-dimension sweep)

> **SCOPE — FULL.** All 23 dimensions of `/audit-renderer`, depth `deep`.
> The `b` suffix distinguishes this from `docs/audits/AUDIT_RENDERER_2026-08-12.md`,
> which is explicitly scoped **PARTIAL** (Dimensions 6 and 7 only, the
> `texture-roles-deep` preset). **This run supersedes nothing.** The same-day
> partial report remains the report of record for its own Dims 6/7 material;
> this report re-derives those dimensions independently at a later HEAD and
> both stand.

- **Date**: 2026-08-12 (evening pass)
- **HEAD at merge**: `316e085e` (*Fix #2464: pin DalcCubeUBO's block size, and every other unpinned UBO*)
- **HEAD at dimension-agent time**: most agents ran at `e4ab12e8`; Dims 9, 13 and 23 ran at `4ae7108b`. The two commits between those and `316e085e` (`4ae7108b` RACE DATA decode, `316e085e` UBO block-size pins) touch `crates/renderer/src/vulkan/reflect.rs`, `bloom.rs` and `ssao.rs` only, and are reconciled inline (see *Issue-status updates*).
- **Depth**: deep — data flows traced, SPIR-V disassembled, shaders recompiled and byte-compared, real Skyrim SE / FO4 archive data decoded where a claim needed measuring.
- **Dedup baseline**: 2672 GitHub issues (open + closed) from `/tmp/audit/renderer/issues_all.json`, plus a full scan of `docs/audits/`.
- **No GPU capture was taken this run.** No engine was launched (standing
  no-parallel-engine-launch rule). Every visual-symptom claim below is stated
  as a **mechanism derived from source**, with a named confirming signal — never
  as an observed image. Nothing in this report proposes a barrier, render-pass,
  or pipeline-dependency edit.

### Prior-run provenance

An orphaned sweep ran at 11:44–11:48 today against `efc089ba` / `8a404914` and
produced ten scratch files but **never a merged report and never a GitHub
issue**. Those scratches are quarantined at `/tmp/audit/renderer/stale_prior_run/`.
Several dimension agents in this run explicitly reconciled against them (Dims 2,
4, 6, 11, 14, 15, 17, 18, 23), re-verifying rather than inheriting each claim.
Those reconciliations are folded in here, and the duplicate pairs are collapsed
to a single finding each — see *Merged duplicates* below. Nothing was suppressed
on the strength of an unpublished scratch file.

---

## 1. Executive Summary

### Deduplicated severity counts

| Severity | Count |
|---|---|
| CRITICAL | **0** |
| HIGH | **8** |
| MEDIUM | **29** |
| LOW | **62** |
| **Total code/doc findings** | **99** |

Separately, and deliberately **excluded** from the counts above:
**9 process & audit-infrastructure findings** (§8) — stale audit instructions
and project-doc rot that corrupt future audits but are not defects in shipped
code.

Dedup work performed on the raw dimension output: one cross-dimension duplicate
merged (`froxel_xy_divisor`, found independently by Dims 5 and 16), two
scratch-run duplicate pairs collapsed (Dim 2 vs. stale D2-01; Dim 11 `-06`/`-07`
vs. stale D11-02/-03), one three-way duplicate reconciled (`water.vert`'s
112-byte comment, raised by Dim 11, Dim 15 and the stale run), and nine
audit-tooling items routed out of the severity counts into §8.

### Findings by pipeline area

| Area | HIGH | MEDIUM | LOW |
|---|---|---|---|
| Acceleration structures / RT (Dims 1, 2) | 1 | 1 | 7 |
| GPU-struct layout & memory lifecycle (Dims 3, 5) | 1 | 4 | 6 |
| Synchronization & command recording (Dims 4, 12) | 2 | 2 | 5 |
| Material translation & table (Dims 6, 7, 17) | 1 | 1 | 6 |
| Denoiser / TAA / composite (Dims 8, 13) | 0 | 3 | 6 |
| Skinning (Dim 9) | 2 | 0 | 3 |
| Precision & render origin (Dim 10) | 1 | 1 | 3 |
| Pipeline state / G-buffer (Dim 11) | 1 | 1 | 5 |
| Caustics & water (Dims 14, 15) | 0 | 4 | 9 |
| Volumetrics & bloom (Dim 16) | 0 | 3 | 1 |
| Sky / weather / exterior (Dim 18) | 0 | 3 | 3 |
| Tangent space & normal maps (Dim 19) | 0 | 3 | 3 |
| Debug overlay & telemetry (Dim 20) | 0 | 1 | 1 |
| Cornell harness (Dim 21) | 0 | 0 | 1 |
| Light animation (Dim 22) | 0 | 0 | 0 |
| FSR 3.1 & presentation (Dim 23) | 0 | 2 | 6 |

**Dimension 22 (light animation canonical translation) returned zero findings.**
The mirrored-pair invariant (`canonical_light_animation_flags` /
`canonical_light_shadow_flags` in `byroredux/src/systems/light_anim.rs`) holds,
every consumer reads the translated value, and the Starfield → `0` arm is pinned
on both sides by one test. It is the only clean dimension in the sweep.

### The one-paragraph read

There is no CRITICAL and no live memory-safety or device-loss defect that a
normal session reaches. What the sweep found instead is **four coherent failure
clusters** (§2) that each span multiple dimensions, plus a long tail of
single-site doc rot. Two clusters — the mesh-ID namespace collision and the
doubly-dead glass caustic pass — are direct fallout from two commits landed in
the last 23 days (`883f57cd`, `c615f8de`) whose consumers were not revisited.
One cluster is a **delivery-integrity failure**: four issues are closed on
GitHub against a commit that is not on `main`, so four defects a prior audit
found are live at HEAD with their tracker entries shut. The fourth cluster is a
pair of far-from-origin f32 precision defects that were latent until this week's
LOD work made them load-bearing.

---

## 2. Root-cause clusters

These are the highest-value output of the sweep. Each was assembled across
dimensions during merge; none is visible from a single dimension's report.

### Cluster A — the `883f57cd` mesh-ID namespace collision

**Three consumers cross-compare two different ID namespaces, separated only by
bit 31, and all three mask that bit off before comparing.**

`883f57cd` (2026-07-20) split the meaning of the low 31 bits of the
`R32_UINT` mesh-ID attachment:

- **opaque** draws write `stableSurfaceId = inst.surfaceId & 0x7FFFFFFFu`, where
  the host sets `surface_id: draw_cmd.entity_id.wrapping_add(1)`
  (`crates/renderer/src/vulkan/context/draw.rs`) — an **ECS entity index**;
- **alpha-blended** draws write
  `sortedInstanceId = (uint(fragInstanceIndex) + 1u) & 0x7FFFFFFFu`
  (`crates/renderer/shaders/triangle.frag`, off
  `fragInstanceIndex = gl_InstanceIndex` in `crates/renderer/shaders/triangle.vert`)
  — a **per-frame sorted draw index**, because `caustic_splat.comp` needs the
  live instance-SSBO lookup.

Bit 31 (`ALPHA_BLEND_NO_HISTORY`) is the *only* discriminator. Both operands are
small dense counters drawn from heavily overlapping ranges, so a match is a
**systematic aliasing condition, not a rare hash collision**: whichever
(entity id, draw index) pair coincides keeps coinciding every frame while both
draws exist, producing a fixed localized artifact rather than noise.

Consumers that mask bit 31 away and then compare the low 31 bits:

| Consumer | Site | Dimension | Bound? |
|---|---|---|---|
| `crates/renderer/shaders/taa.comp` | `disocclusion` predicate + the 5-tap motion-dilation `candidateSurface` test | 13 (REN-D13-01) | No |
| `crates/renderer/shaders/svgf_temporal.comp` | bilinear-tap predicate + sub-pixel-motion fallback | 8 (REN-D8-NEW-01) | Accidentally — see below |
| `crates/renderer/shaders/svgf_atrous.comp` | spatial tap rejection | 8 (REN-D8-NEW-01) | **No** |

`svgf_temporal.comp` is currently self-limiting by accident, and this is worth
recording so a future change does not remove the accident: an alpha-blended
pixel takes the early-out and writes history age 0, and `prevMeshIdTex` /
`prevMomentsHistTex` bind the same `prev` slot, so any tap that trips the false
accept carries `histAge == 0` → `alphaC = 1.0` → the no-history result. The
residue is the *mixed* bilinear case. `svgf_atrous.comp` has no such bound, and
two of its four remaining edge guides are weak in exactly this situation: the
blend pipeline writes the normal attachment with `overwrite`
(`crates/renderer/src/vulkan/pipeline.rs`, `preserve_opaque_gbuffer == false`),
so a camera-facing billboard passes `pow(dot, 128)` against a camera-facing
wall; and alpha-blended draws never write depth, so `wZ` compares the receiver's
depth against itself and evaluates to ~1.

**The mask has outlived its reason.** Dim 8 established that #904/#1159's
motivating case — a single instance toggling between opaque and blended — is no
longer representable: refractive glass now takes the `preserve_opaque_gbuffer`
path and does not write mesh ID at all.

**Interaction with Cluster B.** The same `preserve_opaque_gbuffer` write-mask
(Dim 11, REN-D11-2026-08-12-01) is what removed that representability — and is
simultaneously what kills the caustic pass. The two clusters share one commit
pair and must be reasoned about together.

**Shared fix direction (one change, three sites):** treat *bit 31 set on the
other sample* as an outright **non-match** instead of masking it away. A blended
fragment's low bits are a draw index, not an identity, so no comparison against
them is meaningful. This is a shader-side predicate change observable in
`cargo test` via source-order guards in the style of
`svgf_atrous_stops_on_depth_and_albedo_edges` and
`taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces` — **not** a sync
change, so it needs no capture to land safely.

Also in this cluster's blast radius, as documentation that no longer describes
the code: `crates/renderer/shaders/triangle.frag`'s attachment-3 declaration
comment (REN-D11-2026-08-12-04), the `triangle.frag:1532` line anchor in
`crates/renderer/src/vulkan/context/helpers.rs` (REN-D11-2026-08-12-03), and
`crates/renderer/src/vulkan/gbuffer.rs`'s `MESH_ID_FORMAT` doc-block, which
documents the two representations but not the masking hazard.

---

### Cluster B — the glass-side caustic pass is dead by two independent mechanisms

**The producer and consumer sets for `INSTANCE_FLAG_CAUSTIC_SOURCE` are provably
disjoint, twice over, and the two mechanisms compound: fixing either alone
leaves the pass dark.**

**Mechanism 1 — the write mask (Dim 11, HIGH).** `c615f8de` (2026-08-11) added
the `preserve_opaque_gbuffer` axis to `PipelineKey::Blended` and, when set,
replaced attachments 1–5 — including attachment 3, **`mesh_id`** — with
`no_write` (`crates/renderer/src/vulkan/pipeline.rs`, `create_blend_pipeline`).
The flag is set from `is_refractive_glass(draw_cmd)`. `INSTANCE_FLAG_CAUSTIC_SOURCE`
is set from `is_caustic_source(draw_cmd)`, which is *literally* `is_refractive_glass(cmd)`:

```rust
// crates/renderer/src/vulkan/context/draw.rs
fn is_caustic_source(cmd: &DrawCommand) -> bool {
    is_refractive_glass(cmd)
}
```

`crates/renderer/shaders/caustic_splat.comp` finds sources exclusively through
the mesh-ID attachment (`if ((meshIdRaw & 0x80000000u) == 0u) return;` then
`instIdx = meshId - 1u` then the `INSTANCE_FLAG_CAUSTIC_SOURCE` gate). Glass
*with* alpha-blend has its `outMeshID` discarded by the write mask; glass
*without* alpha-blend never sets bit 31. Every other bit-31 pixel is a non-glass
blended draw and fails the flag gate.

**Mechanism 2 — the CPU gate does not require alpha-blend (Dim 14, MEDIUM).**
`is_caustic_source` consults only `material_kind` and
`multi_layer_refraction_scale`; bit 31 is written from `INSTANCE_FLAG_ALPHA_BLEND`
alone. The shader's own comment asserts the opposite and is wrong:

> *"caustic sources are always alpha-blend (the post-#922 CPU gate restricts
> CAUSTIC_SOURCE to MATERIAL_KIND_GLASS and MultiLayerParallax refraction, both
> of which require alpha-blend upstream)"* — `caustic_splat.comp`

Neither signal requires alpha-blend. `classify_glass_into_material`
(`byroredux/src/helpers.rs`) is gated on `has_transparent_coverage`, which
`translate_material` (`byroredux/src/material_translate.rs`) feeds
`source.has_alpha || source.alpha_test` — **deliberately**, per the classifier's
own doc ("broken-pane sheets use alpha test for shard coverage but still need
dielectric shading"). MultiLayerParallax (kind 11) is accepted on
`multi_layer_refraction_scale > 0.0` with no transparency condition at all. So
alpha-test-only panes and opaque MLP ice/glass are caustic-dead **independently
of Mechanism 1**.

**Why the harness cannot bisect this (Dim 21).** `byroredux/src/cornell.rs`'s
`glass()` probes state outright *"Glass is OPAQUE (no AlphaBlend)"* — the
reference scene built to validate glass exercises the caustic pass **not at
all**, so an engineer reaching for the Cornell harness to answer "why are there
no caustics" gets a clean bisect on a dead feature. This is the same
false-all-clear shape #2477/#2514 were filed to close for the Disney lobe, and
that Dim 21's own REN-D21-01 finds again for the translucency scalars.

**Cost while dark.** The compute pass still dispatches and still pays its full
screen-sized cost every frame in every cell. Water-side caustics are unaffected
(they come from `water.frag`'s own `imageAtomicAdd`).

**Fix ordering matters.** `c615f8de`'s stated rationale was fixing "caustics
through walls", which suggests Mechanism 1 is an over-broad fix rather than an
unnoticed one — but as written the feature is *off*, not corrected. Decide which
contract survives and single-source it: either keep `mesh_id` writable for the
glass pipeline (split the write-mask set so attachment 3 stays `overwrite` while
1/2/4/5 stay `no_write`) and solve the leak in `caustic_splat.comp`'s
depth/geometry gate, or retire the alpha-draw mesh-ID representation and give
the splat an explicit source list. **Then** resolve Mechanism 2 by deciding
whether `is_caustic_source` should require `cmd.alpha_blend` (cheapest — makes
the flag stop lying) or whether opaque glass should cast caustics (a larger
change needing a per-pixel instance index for opaque pixels; file separately).
Update `triangle.frag`, `gbuffer.rs::MESH_ID_FORMAT` and
`docs/engine/shader-pipeline.md` in the same change — all three currently
describe a contract the code no longer has.

A unit test asserting `is_caustic_source(cmd) ⇒ mesh-ID is writable for that
cmd's pipeline key` would have caught Mechanism 1 at `cargo test` time, and
adding an `alpha_blend: false` case to `is_caustic_source_tests` (every fixture
there is currently hard-coded `alpha_blend: true`) would have caught Mechanism 2.

---

### Cluster C — four issues closed as fixed by a commit that is not on `main`

**This is a process failure, not a code-design one, and it needs a merge or a
reopen rather than a fix.**

#2460, #2461, #2462 and #2463 are all **CLOSED** on GitHub, credited to commit
`f3babea3` (*"Fix #2460 #2461 #2462 #2463: AS scratch sizing, RT shading, GPU
struct pin"*, 2026-08-08). Verified at merge time:

```
$ git merge-base --is-ancestor f3babea3 HEAD || echo NOT-ancestor
NOT-ancestor
$ git branch --no-merged main
  fix/2460-2461-2462-2463-as-rt-correctness
  perf/1369-reservoir-spec-constant
$ grep -rn "blas_scratch_peak" crates/renderer/src/          # → 0 hits
$ grep -c "GpuTerrainTile" crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs
0
```

`f3babea3` lives only on `fix/2460-2461-2462-2463-as-rt-correctness`. **All four
defects are live at HEAD.** The two independent confirmations above are the
sharpest: the helper `blas_scratch_peak` that `f3babea3` added to
`crates/renderer/src/vulkan/acceleration/predicates.rs` does not exist anywhere
in the tree, and the `GpuTerrainTile` layout pin the same commit added has zero
occurrences in the file it was added to.

| Issue | Defect, live at HEAD | Severity here |
|---|---|---|
| **#2460** | `shrink_blas_scratch_to_fit` derives its shrink target from `self.blas_entries` only; `self.skinned_blas` is never consulted, though `blas_scratch_buffer` is **one allocation shared** by the static and skinned builders | **HIGH** (REN-D9-2026-08-12-01) |
| **#2461** | `triangle.frag` GI hemisphere viewer-flip | not independently re-derived this run |
| **#2462** | glass passthru `rayTMin` reset | not independently re-derived this run |
| **#2463** | `GpuTerrainTile` has no `size_of`, `offset_of!`, GLSL-lockstep or `.spv` pin (independently re-confirmed by Dim 3) | LOW, tracked |

The #2460 half is the one with teeth. `refit_skinned_blas` performs **no**
scratch-size re-validation — it takes `self.blas_scratch_buffer.as_ref()`, reads
the device address, aligns it, and submits `mode = UPDATE` — so a shrink below a
live skinned entity's `build_scratch_size` is silent. Reachable on window resize
(`crates/renderer/src/vulkan/context/resize.rs`) and cell unload
(`byroredux/src/cell_loader/unload.rs`), where `unload_cell` only *queues*
`pending_skin_unload_victims` (drained a later frame), so the outgoing cell's
skinned BLAS are still resident when the shrink runs. Consequences range from a
corrupted neighbouring `gpu-allocator` slab entry to `VK_ERROR_DEVICE_LOST`.

**These four were themselves the output of a prior audit.** An audit that finds
a defect, files it, gets it fixed on a branch, and sees it closed without
delivery is worse than one that never ran — the tracker now actively asserts the
defect is gone. The structural remedy is a **"fix is reachable from `main`"
gate** in the issue-closure workflow; the immediate remedy is to merge
`fix/2460-2461-2462-2463-as-rt-correctness` (or cherry-pick `f3babea3`) and
re-verify all four closures.

---

### Cluster D — far-from-origin f32 precision, newly load-bearing

**Two independent precision defects that were harmless until this week, and are
now on the critical path because the LOD work extended the reachable distance
regime.**

The enabling changes, all within the last few days:

- `d96110eb` — `MAX_LOD_RING_REACH_CELLS = 61` ⇒ **249 856 BU** per axis of baked
  distant terrain (`byroredux/src/cell_loader/terrain_lod.rs`);
- `9e96a9f9` — `DEFAULT_RENDER_DISTANCE` raised 300 000 → **400 000** so the
  frustum actually reaches it, with a `far² >= 2·reach²` const-assert;
- `14f2e18a` (#2554) — made the ReSTIR reservoir the **entire** direct term at
  every distance, where previously the whole finalize block was gated
  `shadowFade > 0.01` (past `SHADOW_FADE_END` = 12 000 BU the reservoir
  contributed nothing and its reuse quality there was moot).

**D-1 — `cluster_cull.comp` differences two ABSOLUTE positions to get a
0.1-unit vector (Dim 10, REN-D10-01, HIGH).** `ndcToWorld` correctly
reconstructs from the render-origin-**relative** `invViewProj` and then lifts to
ABSOLUTE (`+ renderOrigin.xyz`) so the cluster AABB shares the light SSBO's
space — that part is right. The bug is *when*: the very next use is
`normalize(nearCorners[i] - camPos)`, a small difference of two ~10⁵-magnitude
f32s, and the corners are unprojected at `ndcZ = 0.0`, i.e. the near plane,
which `Camera::default()` puts at 0.1 world units. At Markarth
(`|world| ≈ 176 000`, f32 ULP = 0.015625) the whole near plane is 0.1473 units
wide — 0.0092 units per tile across `CLUSTER_TILES_X = 16`, **below one ULP** —
so adjacent tile boundaries collapse onto the same f32 and those tiles build a
zero-width frustum voxel. Degradation is gradual and origin-magnitude-driven:
~10 % of a tile at 16 k, ~42 % at 65 k, total collapse at ≥ 131 k. Interiors are
unaffected, which is why it has not been noticed.

**D-2 — the ReSTIR reservoir depth lane saturates on write but not on read
(Dim 2, REN-D2-01, LOW).** The write correctly saturates
(`min(worldDist, 65504.0)`, because `packHalf2x16` of anything larger yields
`+Inf`, not a clamp); the read compares `rnWorldDist` against the **unclamped**
`worldDist`. For a fragment past 65 504 the residual is at least
`worldDist - 65504` for *every* neighbour whatever its true depth, so reuse
requires `worldDist ≤ 65504 / 0.98 = 66 840.8`. **Past ~66 841 BU the spatial
pass rejects 100 % of its five taps at every pixel** — not because neighbours
are incompatible, but because one side of the comparison is saturated and the
other is not. Silent: the loop still runs, still costs five `Reservoir` loads
plus the oct-decode, and contributes nothing. Not a precision problem — half
spacing at 32 768 is 32 BU against a 2 % tolerance of 655 BU — purely a clamp
asymmetry.

**Why they belong together.** Both are "the arithmetic was fine in the regime we
could reach"; both became reachable in the same week from the same LOD work;
neither is caught by `cargo test`, by any interior smoke test, or by the
validation layers. D-1 costs point/spot lights in per-tile patches in exteriors
(directional is unaffected — `lightType > 1.5` → `intersects = true`
unconditionally — so the sun is safe and the symptom is lamps/campfires/neon
dropping out at night). D-2 costs variance reduction on exactly the
newly-extended distant ring.

Both fixes are pure arithmetic reordering with no descriptor, layout or barrier
implication: for D-1, take the difference in relative space and lift once
(`camRel = cameraPos.xyz - renderOrigin.xyz` is exact in f32 — the origin is a
floor-multiple of `RENDER_ORIGIN_SNAP`); for D-2, compare against
`min(worldDist, 65504.0)`, ideally through a named local shared with the write
site so the two cannot drift again.

**Related, same class, already correct — do not re-break:** #1490
(`composite screen_to_world_dir`), #1642 (soft-particle depth fade), #1488
(caustic re-projection), #1997/#2469 (water procedural-noise rebase). Dim 10
also flags `getHitTriWorldPositions` (REN-D10-02, MEDIUM) as the *latent* member
of this family — its rigid branch returns RELATIVE and its skinned branch
ABSOLUTE, under a name and doc that both promise absolute. No wrong pixels today
because every consumer uses only differences, but the next caller that wants a
hit *position* inherits a silently branch-dependent frame. That is exactly what
#1488 shipped.

---

### Merged duplicates

**`froxel_xy_divisor` 12 → 8 — found independently by two dimensions, with
identical numbers. Reported once; the independent confirmation strengthens it.**

- Dim 5 filed it as **REN-D5-02** (MEDIUM) from the memory-lifecycle side.
- Dim 16 filed it as **REN-D16-02** (MEDIUM) from the volumetrics side, and
  additionally classes it a **regression of #2230** — CLOSED by `583e0ae7`,
  which is the commit that wrote the now-stale `/12` formula. That
  classification is carried.

Both derived the same figures from the same source of truth: `5798e467`
(2026-08-09) changed `VolumetricsConfig::default().froxel_xy_divisor` from 12 to
8 in `crates/renderer/src/vulkan/upscaling.rs` and updated no derived number.
Footprint scales with the square of the divisor, so the error is exactly
`(12/8)² = 2.25×` — **66.4 MB actual vs. 29.5 MB documented at 1080p**. Filed
once below as **REN-D5-02 / REN-D16-02**.

Other duplicates the agents themselves flagged, verified and filed once:

| Pair | Resolution |
|---|---|
| Dim 2 `REN-D2-03` vs. stale `REN-D2-01` (`shader-pipeline.md` Set-1 drift) | Same item. Dim 2 re-verified at HEAD rather than inheriting. Filed once as **REN-D2-03**. |
| Dim 11 `-06`/`-07` vs. stale `REN-D11-02`/`-03` | Same items. Filed once as **REN-D11-2026-08-12-06** (`gbuffer.rs` "five" vs. seven) and **-07** (`water.vert` 112-byte comment). |
| Dim 15 `REN-D15-06` vs. Dim 11 `-07` vs. stale `REN-D11-03` | Three-way on the same `water.vert` comment block. Filed once as **REN-D11-2026-08-12-07**; Dim 15's water-specific framing is folded into it. |
| "#2508 binding 8 aliases binding 5" | Dim 14 reported a *third* stale copy beyond the stale run's two. Grep at merge confirms two exact-phrase sites — `crates/renderer/shaders/composite.frag` and `crates/renderer/src/vulkan/composite.rs` (`CompositeParams::caustic_flags` doc) — plus the looser sibling note at the `waterCausticTex` declaration. **Fix all together**; the fallback is now a dedicated 1×1 `placeholder_caustic_sink`, and it *could not* alias binding 5, which became a `TYPE_2D_ARRAY`. |

---

## 3. RT Pipeline Assessment

**Verdict: the acceleration-structure layer is the healthiest part of the
renderer. Every AS-correctness invariant on the Dimension 1 checklist holds. The
RT problems this sweep found are one delivery failure (Cluster C), one precision
defect in the *clustered-lighting* side of the shader (Cluster D-1), and one
dead consumer (Cluster B) — none of them in AS lifecycle itself.**

### BLAS / TLAS lifecycle — intact

Re-verified at HEAD, not carried from prior reports:

- **Geometry + flags.** `vertex_format(R32G32B32_SFLOAT)`, `index_type(UINT32)`,
  `GeometryFlagsKHR::OPAQUE` at all three build sites and at
  `refit_skinned_blas`. `Vertex::position` is still the first field of the
  `#[repr(C)]` struct, so the format is valid at offset 0 with a 104 B stride;
  the skinned path correctly strides by `SKIN_OUTPUT_STRIDE_BYTES` (12 B,
  position-only) instead — #2170's split holds.
  `max_vertex = vertex_count.saturating_sub(1)` at all four geometry sites.
- **BUILD vs UPDATE.** The `instance_count != built_primitive_count → force
  BUILD` guard (VUID-03708) is present with its `debug_assert_eq!` on the UPDATE
  arm. `BlasEntry.built_flags` records BUILD-time flags and `refit_skinned_blas`
  validates both halves of VUID-03667 (`validate_refit_flags` +
  `validate_refit_counts`) before issuing UPDATE, with a drop-and-rebuild
  fallback rather than a silent violation.
- **The commit-point discipline landed today holds.** `4659cbe0` (#2673/#2674)
  is verified intact from three dimensions independently (1, 4, 5):
  `ensure_tlas_state` builds replacement buffers, AS and scratch into locals and
  retires the old slot only past a non-failing commit point;
  `build_tlas` promotes `last_blas_addresses`, clears `needs_full_rebuild` and
  stamps `last_blas_map_gen` **after** `cmd_build_acceleration_structures`.
  Both are source-position pinned. The TLAS use-after-free is **fixed, and is
  recast here as a verified guard rather than re-reported.**
- **Deferred destruction.** `drop_blas`, `evict_unused_blas` and
  `drop_skinned_blas` all route through `pending_destroy_blas` with a
  `MAX_FRAMES_IN_FLIGHT` countdown; no immediate
  `destroy_acceleration_structure` was reintroduced at any eviction site.
  #1782's `pending_destroy_scratch` route is intact at all four retire sites.
- **Empty TLAS / frame 0.** Zero-instance frames still take the `copy_size > 0`
  guard (skipping both buffer barriers and the copy, #317) while still issuing a
  legal `primitiveCount = 0` BUILD, so descriptor binding 2 is valid from frame 0.
- **Scratch alignment.** `scratch_alignment_padding` headroom is added at every
  allocation and `align_scratch_address` rounds at every use; grow checks compare
  against the *padded* requirement, so a shrink path reallocating at the unpadded
  peak is self-correcting rather than a latent overrun.

### The AS ↔ SSBO index contract (the CRITICAL surface) — intact

`instance_custom_index` is `vk::Packed24_8::new(ssbo_idx, shadow_mask)` where
`ssbo_idx` comes from the shared `instance_map`, never a raw enumerate index.
Dim 1 re-read the whole builder span and confirmed the `keep` predicate at the
`build_instance_map` call site is byte-identical in effect to the SSBO builder
loop's sole `continue`, and that the UI-quad push happens *after* the main loop
so draw-command SSBO positions are untouched. `MAX_INSTANCES = 0x40000` under
`const _: () = assert!(MAX_INSTANCES < (1 << 24))`, mirrored by the
`debug_assert!` at the truncation site; over-cap draws are forced to `None` by
`build_instance_map`'s `max_kept`, so no TLAS instance can name an unuploaded
SSBO slot.

Dim 2 independently confirmed the shader half: **all six** RT hit sites index
`instances[]` by `rayQueryGetIntersectionInstanceCustomIndexEXT`, never
`gl_InstanceID` — `crates/renderer/shaders/include/raytrace.glsl` (`traceReflection`),
`crates/renderer/shaders/include/shadow_transport.glsl` (×2), `triangle.frag` (refraction and GI loops),
and `water.frag` (`traceWaterRay`). `gl_InstanceIndex` appears only on raster
paths. Every RT vertex/index fetch goes through `crates/renderer/shaders/include/ray_hit.glsl` and
strides by the generated `VERTEX_STRIDE_FLOATS` with named offset constants — no
open-coded strides at any hit site.

### Ray-query safety — intact, with two hardening gaps

- **Ray origin bias.** `offsetRayOrigin` (Wächter & Binder, ULP-scaled) with
  `tMin = 0.0` for shadow rays on a view-corrected *geometric* normal;
  `tMin = 0.05` for reflection / refraction / GI matching their callers' 0.05/0.1
  `N_bias`. The window portal still starts at `-N * 0.15` with raw `N` (the
  deliberate documented asymmetry, #821). **No fixed-epsilon regression at large
  `|world|`** — which matters now that the far plane is 400 000.
- **Cluster indexing is bounded and the two shaders agree.**
  `cluster_cull.comp` writes `offset = clusterIdx * MAX_LIGHTS_PER_CLUSTER` and
  `count = min(sharedCount, MAX_LIGHTS_PER_CLUSTER)` after an atomicAdd-then-
  bounds-check append, so `clusterLightIndices[cluster.offset + ci]` for
  `ci < count` can never read another cluster's slots. `getClusterIndex` and
  `sliceDepth` slice on world-space distance with the same `clusterFar()`
  expression and the same `CLUSTER_NEAR`; all three axes are `min`-clamped.
  *(The **construction** of the per-tile frusta is what Cluster D-1 breaks — the
  indexing is sound.)*
- **RT gating cannot be bypassed.** `rtEnabled = sceneFlags.x > 0.5 &&
  !compileDisableAllRays && !runtimeDisableAllRays`, with the three per-feature
  enables AND-ed *after* it. `render_debug_flags` is written exactly once, at
  `VulkanContext::new`, so the runtime `DBG_*` bits are launch-time-immutable.
- **Gap 1 (REN-D2-02, LOW):** the 10-bit ReSTIR light lane
  (`RESERVOIR_LIGHT_MASK = 0x3FFu`) has no lockstep guard against `MAX_LIGHTS`,
  and the two constants are **structurally unable to see each other** — the mask
  is a hand-written GLSL literal, and `MAX_LIGHTS` is `pub(super)` inside
  `scene_buffer`, so `crates/renderer/src/vulkan/restir.rs` (where the
  reservoir's other pins live) cannot name it. Correct today only because
  511 < 1023. If `MAX_LIGHTS` were raised the failure is not a clean overflow:
  index 1024 aliases to 0 and 1023 becomes indistinguishable from the invalid
  sentinel, so the shadow reservoir silently selects the **wrong light** — which
  under the severity table is a CRITICAL-floor SSBO index mismatch. Only its
  unreachability today holds it at LOW.
- **Gap 2 (Cluster D-2):** the reservoir depth-lane clamp asymmetry, above.

### Denoiser stability

- **SVGF.** History ping-pong derives every previous-frame binding from one
  `prev = (f + 1) % MAX_FRAMES_IN_FLIGHT`, so the disocclusion test and the
  history it gates cannot disagree about which frame they read. The firefly-
  rejection hoist (`48906670`) still sits ahead of `if (hasHistory)`, so the
  no-history disocclusion path is clamped too. The à-trous chain's
  `ATROUS_ITERATIONS = 3` odd-count compile assertion still makes
  `atrous_final_pp() == 0` the slot composite reads. Dispatch is exact-coverage
  `div_ceil(8)` with in-shader bounds checks on both passes.
- **TAA.** The mandated guard
  `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces` passes and all
  four of its assertions still match live shader text. #1497's `static_frames`
  progressive alpha floor is confirmed **absent and unable to recur**
  (`upload_params` computes a flat `alpha = 0.1`). Motion vectors are jitter-free
  on both consumers (`fragCurrClipPos` assigned *before* the jitter add), NaN/Inf
  pre-filter runs before the YCoCg clamp, and the first-frame reject sits above
  every history fetch.
- **The two stability defects found are both Cluster A** (mesh-ID cross-namespace
  matching), plus one architectural TAA gap: **REN-D13-02**, geometry
  silhouettes against the sky can never accumulate history in either jitter
  state, because sky is synthesised inside `composite.frag` and is not in TAA's
  input at all. With a parked camera the motion vector is exactly zero, so the
  pixel re-disoccludes itself every frame — a *permanent* disocclusion, not the
  one-frame kind the test is designed for. Enabling TAA makes horizon edges
  *worse* than leaving jitter off. Reduced exposure: `UpscalerMode::default()` is
  `Fsr3(Quality)`, so TAA is the opt-in path.

### RT-adjacent items requiring no action

`#1793`'s two documented AS/budget gaps (no per-frame recovery primitive for a
permanently-missing rigid BLAS; a synchronous multi-cell `--grid` burst can
false-age a not-yet-drawn entry) are unchanged, still gated behind
`static_blas_bytes > budget`, and unreachable on the 12 GB dev card.
`#1228`'s three `missing_blas` cause counters still surface only through a
rate-limited `log::warn!`. `#1438`'s deliberate `atomicAdd` budget overshoot is
by design with its rationale intact. All recast as guards, not re-reported.

---

## 4. GPU-Struct & Memory Assessment

**Verdict: zero layout drift. Every `#[repr(C)]` GPU struct matches its GLSL
mirror and the shipped SPIR-V, verified three independent ways. The findings in
this area are all about *guards that do not exist* and *ledgers that are wrong*,
not about live corruption. The one HIGH is a teardown-hygiene defect on a
failure path.**

### Layout pins — no drift

Dim 3 validated three ways, deliberately not trusting size-only pins: field-by-
field Rust extraction; every GLSL declaration site; and `spirv-dis` on the
**committed** `.spv` extracting `OpMemberDecorate … Offset` and
`OpDecorate … ArrayStride` — i.e. what the GPU actually reads, not what the GLSL
text says.

| Struct | Rust `size_of` | SPIR-V evidence | Mirrors |
|---|---|---|---|
| `GpuInstance` | 128 B | `ArrayStride 128`; `surfaceId`@108, `skinnedVertexAddress`@112, `_reserved`@120 | 5 GLSL sites, 6 `.spv`, all identical |
| `GpuMaterial` | 348 B | `ArrayStride 348`; **all 87** members, offsets 0…344 | `triangle.frag.spv` + `water.frag.spv` byte-identical member tables |
| `GpuCamera` / `CameraUBO` | 336 B | last member `renderOrigin`@320 | all 6 re-declarers |
| `GpuLight` | 64 B | `ArrayStride 64`; `params`@48 | 4 sites |
| `GpuTerrainTile` | 96 B | `ArrayStride 96`; members @0/32/64 | 2 sites — **layout correct, but unpinned (#2463, Cluster C)** |
| `Reservoir` | 32 B | `ArrayStride 32`; `pad0`@28 | 2 sites |

`GpuMaterial` is 87 × 4 B scalars with **zero** implicit padding (348 = 87×4) —
no `[f32;3]` anywhere — so the byte-`Hash`/`Eq` dedup path has no uninit bytes.
Rust `// offset N` comments, the `offset_of!` assertions, GLSL declaration order
and SPIR-V `Offset` decorations agree at all 87 positions.

**All 21 first-party shaders recompile byte-identical** to their committed
`.spv` under the CLAUDE.md-documented `glslangValidator -V` (confirmed
independently by Dims 2, 3, 9, 13 and 15), and `cargo build -p byroredux-renderer`
regenerates `crates/renderer/shaders/include/shader_constants.glsl` with zero
working-tree drift. `cargo test -p byroredux-renderer --lib` → **580 passed,
0 failed**.

### The real exposure: unequal guard strength across the four hand-mirrored structs

**REN-D3-2026-08-12-01 (MEDIUM)** is the structural finding here. `GpuInstance`
has the *largest* mirror fan-out of any GPU struct (5 sites) and the documented
recurring trap is precisely a mirror reading wrong offsets (#785 / #1498, both
on `ui.vert` / `water.vert`) — yet its only cross-mirror guard is a
`src.contains()` needle check on three field names. Its two siblings already have
exactly the guard it lacks, and **all of that machinery lives in the same file**:
`gpu_light_glsl_copies_stay_in_lockstep` (#1916) does full stripped-body equality
across four copies; `gpu_material_glsl_field_order_matches_rust_struct` (#1657)
does a full ordered Rust↔GLSL comparison.

The worked failure Dim 3 constructed is the point: delete `float ior;` from
`ui.vert`'s mirror and the **stride stays 128 B** (`skinnedVertexAddress`
re-aligns to 8), so no stride check catches it either, while `surfaceId` now
reads `avg_albedo_b`'s bytes — and every assertion in the test still passes. CI
does not backstop this: `scripts/check-shader-artifacts.sh` proves each `.spv`
is reproducible from its GLSL, which says nothing about whether that GLSL
matches the Rust struct, and SSBO `ArrayStride` has no reflection helper at all.

Related, one rung lower and now partly closed: **#2464** (`DalcCubeUBO` block
size unpinned) was **fixed at HEAD by `316e085e`**, which landed after every
dimension agent ran — see *Issue-status updates*. **#2463** (`GpuTerrainTile`
unpinned) remains open at HEAD via Cluster C. **#2688** (lockstep pins names,
order and Rust offsets but never the GLSL scalar *type*) remains open, and Dim 3
sharpened its premise: the SPIR-V offsets extracted this run are type-derived, so
a `uint`↔`float` swap at equal size stays invisible to `cargo test`.

### Semantic lockstep — one live break

**REN-D3-2026-08-12-02 (MEDIUM):** `GpuCamera::dof_params.zw` is documented
*"reserved (0)"* in the Rust struct and in 4 of the 5 GLSL `CameraUBO` mirrors,
while both lanes carry live data (`.z` = `light_atten_knee`, consumed by
`crates/renderer/shaders/include/lighting.glsl`; `.w` = `camera_static`, consumed by `triangle.frag` for
both the parked-camera branch and the GI-seed decorrelation). Byte layout is
correct everywhere — this is a semantic break, and the wording is not merely
stale but an **active invitation** to repurpose a lane with two consumers. The
authoritative `docs/engine/shader-pipeline.md` is already correct, so this is
code-side divergence from a doc that got fixed, not doc rot. Exactly the trap the
codebase already burned on with `_pad_id0` → `ior` (#2164).

### Memory lifecycle

**Per-frame leak sweep: none found.** Every allocation site reachable from
`draw_frame` is create-once (scene SSBOs, screen-sized passes), pool-recycled
(`StagingPool`), refcounted (`MeshRegistry`, `TextureRegistry`), or
LRU/countdown-bounded (`skin_slots`, `pending_destroy_blas`,
`pending_destroy_scratch`, `MeshRegistry::deferred_destroy`). The one live
per-frame-allocation issue in the renderer is already filed (#2719, UI overlay
`VkImage` per frame) and sits outside this dimension's entry points.

**Teardown ordering: correct end to end.** `VulkanContext::drop` opens with
`device_wait_idle`, and the `#2406` split into
`destroy_allocator_owned_resources` preserved the allocator-independent hoist:
`egui_pass`, `presentation`, `gpu_timers`, `skin_palette`, `water` and
`frame_upscaler.destroy_device_objects` all run before the allocator guard, on
every Drop path. Framebuffers precede the render pass; image views precede the
swapchain; the allocator is dropped before the device.
`SceneBuffers::destroy` and `AccelerationManager::destroy` were both walked
field-by-field — no orphans.

**The HIGH here is a failure-path defect, not a steady-state one.**
**REN-D5-01**: `CausticPipeline` / `TaaPipeline` / `SvgfPipeline::destroy` are
non-idempotent — each guards on `if self.pipeline != vk::Pipeline::null()` but
**never writes the handle back to `null()` after destroying it**, so the guard is
never armed for a second pass. All three `recreate_on_resize` implementations
end their partial-failure arm with `self.destroy(...)` and then propagate the
error through `?`, leaving the context field `Some`, so
`destroy_allocator_owned_resources` destroys them again. Confirmed at merge by
counting null-assignments: `caustic.rs` 0, `taa.rs` 0, `svgf.rs` 0, versus
`presentation.rs` **8** — the in-repo model that gets it right. Trigger is an
allocation failure inside a swapchain resize, i.e. exactly the memory-pressure
condition the rest of this machinery exists to survive.

**Two GPU-handle leaks on early returns (REN-D5-03, MEDIUM):**
`spawn_object_lod_quad` (`byroredux/src/cell_loader/object_lod.rs`) and
`spawn_placement_lod_cell` (`byroredux/src/cell_loader/placement_lod.rs`) both
acquire refcounted GPU resources into locals and bail through an
`entities.is_empty()` guard that discards them. `World::despawn` has no GPU side
effects and the returned block is `None`, so no downstream path can perform the
matching release. The correct sibling is right next door and says so explicitly:
`byroredux/src/cell_loader/terrain_lod_btr.rs`'s upload-failure arm drops both
handles with the comment *"or a failed upload pins their VkImages + bindless
slots for the session (the #1537 leak shape)"*.

### VRAM ledger accuracy — three independent divergences, all understating

`docs/engine/memory-budget.md` is the document `_audit-common.md` designates
authoritative for VRAM ceilings, against a 6 GB RT minimum and a "< 4 GB total"
design target. Three separate rows are wrong, all in the same direction:

| Row | Documented | Actual | Ratio | Finding |
|---|---|---|---|---|
| Volumetrics froxel grid (1080p) | ~29.5 MB | **~66.4 MB** | 2.25× | REN-D5-02 / REN-D16-02 |
| Bloom pyramid (1080p) | ~3.5 MB, *"not FIF-doubled"* | **~11.0 MB**, one pyramid per FIF | ~3.2× | REN-D16-03 |
| Glass-caustic RGB array + SVGF à-trous ping-pong | — | — | — | **#2679, already filed, still unfixed** |

The bloom row's prose is the more dangerous of the two new ones: *"not
FIF-doubled, unlike everything else on this page"* is a **wrong invariant**, and
being per-FIF is in fact *required* — `dispatch()` rewrites
`down_descriptor_sets[0]` binding 0 every frame and writes every mip with no
pre-barrier, which is only sound because each slot's images are exclusive to that
slot. A future refactor citing that sentence to collapse `frames` to one pyramid
would reintroduce the cross-frame WAR that #931's barrier reduction depends on
being absent.

Two smaller in-code arithmetic rots in the same family: **REN-D5-04**, the
bind-inverse staging buffer is described in situ as *"≈ 144 KB"* when the
constant makes it **12.6 MB** — an 87× understatement next to the second-largest
host-visible allocation the renderer makes; and **REN-D5-06**,
`crates/renderer/src/deferred_destroy.rs`'s module doc claims two production
users when there are three, omitting `pending_destroy_scratch` — which *is* the
#1782 GPU use-after-free fix, so a reader auditing deferred-destroy coverage from
that doc reaches the exact wrong conclusion that produced #1782 in the first
place.

**Recommendation carried from #1814:** the only mechanism that has actually
stopped this drift recurring is a one-line `log::info!` size report at the
allocation site. Volumetrics and bloom should both get one.

### Host/device coherency — one HIGH, one MEDIUM, one LOW, all the same buffer

Dims 4 and 5 independently reached the `image_health_buffers` readback added
yesterday by `9d63737d` (#2736) and found three coupled problems, which must move
together or not at all:

- **REN-D4-04 (HIGH):** the readback performs no availability operation and no
  `vkInvalidateMappedMemoryRanges`. `grep -rn "HOST_READ\|invalidate_mapped"
  crates/renderer/src` → **zero hits**; all six `PipelineStageFlags::HOST` uses
  are `HOST` as *source* paired with `HOST_WRITE`. The codebase already models
  the non-coherent case — but only for host→device writes.
- **REN-D4-05 (MEDIUM):** the buffer is allocated `MemoryLocation::CpuToGpu` (an
  upload location) while the repo's only other readback correctly uses
  `GpuToCpu`. It is a readback buffer wearing an upload buffer's allocation.
- **REN-D5-05 (LOW):** documents *why* this is benign today — gpu-allocator
  0.27 puts `HOST_COHERENT` in both the preferred and the **required** flag sets
  for `CpuToGpu`, so `is_coherent` is true by construction — and notes that
  nothing in the source says so.

The coupling is the finding: switching to the semantically-correct `GpuToCpu`
(which prefers `HOST_CACHED`) without first adding the availability step would
make the latent bug **more** likely to bite, not less.

---

## 5. Findings

Dimension finding IDs are preserved **verbatim** so the scratch files at
`/tmp/audit/renderer/dim_N.md` stay traceable.

### CRITICAL

**None.** No finding in this sweep meets the CRITICAL floor at HEAD. Two come
close and are held down only by reachability: REN-D2-02's 10-bit light lane
would be a CRITICAL-floor SSBO index mismatch if `MAX_LIGHTS` were ever raised,
and REN-D12-2026-08-12-01's unclamped indirect draw is a device-lost-class spec
violation gated behind a draw count ~20× the densest cell the codebase measures.

---

### HIGH (8)

#### REN-D4-01: `recreate_for_swapchain`'s fence loop destroys before a fallible recreate with no null-out, and the `#1211` sentinel does not cover that step

- **Severity**: HIGH
- **Dimension**: 4 — Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/sync.rs` — `recreate_for_swapchain`, the `for fence in &mut self.in_flight` loop; `crates/renderer/src/vulkan/context/resize.rs` — `recreate_screen_passes`
- **Status**: NEW. Raised by the quarantined 11:45 pass, **re-verified present at `e4ab12e8`** by Dim 4 and carried here so it is published once. Never filed as a GitHub issue.
- **Description**: The `in_flight` loop does `destroy_fence(*fence)` and then a fallible `create_fence(...)?` **without nulling the handle first**, while the `render_finished` loop directly above it correctly `clear()`s before rebuilding. If `create_fence` fails mid-loop, the error propagates with `self.in_flight` holding one or more destroyed handles that later code — including `VulkanContext::drop` — will use or destroy again. Compounding it, `recreate_screen_passes` assigns `self.framebuffers = create_main_framebuffers(...)` **before** calling `recreate_for_swapchain(...)?`, so the `#1211` `framebuffers.is_empty()` sentinel (which is what makes a partially-failed resize survivable) is already satisfied by the time the fence step can fail.
- **Evidence**: Dim 4 re-read both functions at `e4ab12e8`; the asymmetry between the two loops in the same function is the direct evidence. The ordering in `recreate_screen_passes` is source-visible: framebuffer assignment precedes the sync recreate.
- **Impact**: Use of a destroyed `VkFence` after a failed swapchain recreate — a spec violation with driver-defined consequences, and a double-destroy at teardown. Reachable only when `vkCreateFence` fails, i.e. under host-memory pressure during a resize. Failure-path only, which is why it is HIGH rather than CRITICAL.
- **Related**: #1211 (the sentinel), #910 / #952 / #1188 (the sibling error-recovery paths that *are* correct), REN-D5-01 (the same no-null-after-destroy hygiene class one layer up).
- **Suggested Fix**: Mirror the `render_finished` loop — `clear()` or null each handle before the fallible recreate — and move the `framebuffers` assignment after `recreate_for_swapchain` so the `#1211` sentinel covers the whole function. Pure host-side state; unit-testable device-free in the style of the existing `sync.rs` tests. **No barrier or render-pass change is implied.**

#### REN-D4-04: the two device→host readbacks make no availability/invalidate step, and the fence-is-sufficient claim contradicts the Vulkan spec

- **Severity**: HIGH (Vulkan spec violation — `_audit-severity.md` special-rule floor)
- **Dimension**: 4 — Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/context/resources.rs` — `VulkanContext::collect_image_health`; `crates/renderer/src/vulkan/context/draw.rs` — the `collect_image_health(frame)` call after the both-slots `wait_for_fences`; `crates/renderer/src/vulkan/context/screenshot.rs` — `screenshot_finish_readback` (the pre-existing sibling). Contrast: `crates/renderer/src/vulkan/buffer.rs` — `GpuBuffer::is_coherent` / `flush_mapped` / `flush_range`.
- **Status**: NEW
- **Description**: `#2736` (`9d63737d`) added a per-frame device→host readback: `presentation.frag` `atomicAdd`s into a host-visible storage buffer, and `collect_image_health` reads and zeroes it on the CPU right after the frame slot's fence wait. Its justification is repeated in three places — the commit body, the `image_health_buffers` field doc, and the call-site comment — as *"The fence wait above proves that submission completed, so the buffer is provably idle: reading and clearing it on the CPU needs no barrier, no transfer and no extra synchronisation."* The premise is right and the conclusion does not follow. The spec's fence section says the opposite in as many words: *"Signaling a fence and waiting on the host does not guarantee that the results of memory accesses will be visible to the host, as the access scope of a memory dependency defined by a fence only includes **device** access."* What makes this a finding rather than a quibble is that **this codebase already models the non-coherent case, but only for the opposite direction** — `GpuBuffer` caches `is_coherent` and conditionally issues `vkFlushMappedMemoryRanges` for host→device writes, with an atom-size test to go with it. There is no `invalidate_mapped` counterpart, so the one direction that needs it is the one direction that never got it.
- **Evidence**: `grep -rn "HOST_READ\|invalidate_mapped" crates/renderer/src` → **zero hits**. `grep -rn "PipelineStageFlags::HOST" crates/renderer/src` → six hits (`draw.rs` ×2, `ssao.rs`, `caustic.rs`, `volumetrics.rs`, `crates/renderer/src/vulkan/acceleration/tlas.rs`), every one `HOST` as *source* paired with `AccessFlags::HOST_WRITE`; none is a destination stage. `collect_image_health` reads `bytes[0..8]` straight out of `mapped_slice_mut()` with no coherence check, unlike `GpuBuffer::flush_range` which gates on `self.is_coherent`.
- **Impact**: On a memory type that is `HOST_VISIBLE` but not `HOST_COHERENT` (or `HOST_CACHED` without invalidation), the host read may return stale cache lines. Blast radius is bounded — both readers are diagnostics — but they are diagnostics with teeth: `image_health` feeds the `ImageHealth` ECS resource, the bench summary, and the exterior smoke gate's hard-fail on a non-zero running total (`docs/smoke-tests/m-exteriors.sh`). A stale read silently converts a NaN gate into a gate that passes anything; the screenshot path can hand a stale or torn frame to the golden-frame comparison for the same reason. On the RTX 4070 Ti dev card gpu-allocator will pick a coherent type and neither will misbehave — which is exactly why this needs a device to falsify rather than a unit test.
- **Related**: #2736; #2484 (the other open access-scope finding on this frame tail); REN-D4-05 and REN-D5-05 (the same buffer, the allocation-location and documentation halves).
- **Suggested Fix**: **Do not blind-fix.** The code half is quarantined to §7 pending a `BYRO_VALIDATION=1` run with synchronization validation enabled. The **documentation** half is safe to correct independently and should be: the three comments asserting a fence is sufficient for host visibility state something the spec explicitly denies, and that claim will be copied by the next readback path someone adds.

#### REN-D5-01: `CausticPipeline` / `TaaPipeline` / `SvgfPipeline::destroy` are non-idempotent, yet all three self-`destroy()` on a failed `recreate_on_resize` while the context field stays `Some`

- **Severity**: HIGH
- **Dimension**: 5 — Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/caustic.rs`, `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/src/vulkan/svgf.rs` (each `destroy` + `recreate_on_resize`); callers `crates/renderer/src/vulkan/context/resize.rs` (`recreate_screen_passes`) and `crates/renderer/src/vulkan/context/mod.rs` (`destroy_allocator_owned_resources`, reached from `impl Drop for VulkanContext`). Correct in-repo model: `crates/renderer/src/vulkan/presentation.rs` — `PresentationPipeline::destroy`.
- **Status**: NEW (no OPEN or CLOSED issue matches; nearest is #2685, the `EguiPass` render-pass leak on *its* recreate error path — sibling failure class, different site)
- **Description**: All three `recreate_on_resize` implementations end their partial-failure arm with `unsafe { self.destroy(device, allocator) }; return result;`. `recreate_screen_passes` invokes each with `?`, so the error propagates out of `recreate_swapchain` **without the field ever being set to `None`**. `destroy_allocator_owned_resources` then runs `if let Some(ref mut svgf) = self.svgf { svgf.destroy(...) }` (same for `caustic`, `taa`) — a second `destroy()` on the same object. That is only sound if `destroy()` is idempotent. It *is* for the image state (`slots.drain(..)`, `indirect_history.drain(..)`, `atrous_color.drain(..)`, `param_buffers.clear()` all empty their containers), but **not** for the scalar handles: each is guarded with `if self.pipeline != vk::Pipeline::null() { … }` and **none writes the handle back to `null()` after destroying it**, so the guard is never armed for the second pass.
- **Evidence**: Counted at merge time — `grep -c '= vk::.*::null();'` gives **caustic.rs 0, taa.rs 0, svgf.rs 0, presentation.rs 8**. `presentation.rs` nulls every handle after destroying it; `frame_upscaler.rs` and `gbuffer.rs` are safe for a different reason (they own no scalar handles at all — everything is drained or `take()`n); `bloom` and `volumetrics` avoid the class entirely in `recreate_screen_passes` by doing `take()` / `= None` **before** destroying. So six of nine sites are safe and three are not.
- **Impact**: Double `vkDestroyPipeline` / `vkDestroyPipelineLayout` / `vkDestroyDescriptorPool` / `vkDestroyDescriptorSetLayout` / `vkDestroySampler` (plus SVGF's four `atrous_*` siblings) — a spec violation (`VUID-vkDestroyPipeline-pipeline-parameter` and siblings require a valid or `VK_NULL_HANDLE` handle) and a driver-side double-free at process teardown. Trigger is a VRAM/host allocation failure inside `create_slot` / `create_history_image` during a swapchain resize.
- **Related**: #2685 (same "partial recreate failure leaves corrupt state" class); #2487 (`GpuBuffer::destroy` leaves a dangling `self.buffer` — the same hygiene gap one level down); #1211; REN-D4-01.
- **Suggested Fix**: Null each handle immediately after destroying it in all three `destroy()` bodies, mirroring `PresentationPipeline::destroy`. This is host-side state only and carries **no barrier or render-pass risk**. A source-scan unit test in the style of `resize.rs::old_image_views_destroyed_between_new_swapchain_creation_and_old_destroy` can pin the pattern without a device. *(Confirming the current failure would need an induced-OOM run under validation layers — see §7 — but the fix itself does not.)*

#### REN-D6-2026-08-12-01: the `SkinTint | HairTint` arm intercepts every Skyrim body/hands material before the model-space-normal slot-7 specular rule can run — 394/394 authored `_S.dds` masks dropped

- **Severity**: HIGH
- **Dimension**: 6 — NIFAL Material
- **Location**: `crates/nif/src/import/material/dedicated_shader.rs` — the `5 | 6 =>` arm of the `match shader.shader_type` block, versus the `model_space_normals && info.specular_map.is_none()` slot-7 read that lives only in the `_ =>` default arm. Sink: `MaterialInfo::specular_map` → `MaterialTextureSet::specular` → `GpuMaterial::specular_map_index` → `crates/renderer/shaders/triangle.frag`.
- **Status**: NEW (no issue mentions slot 7, SkinTint, or a dropped specular map; not covered by the #2693–#2697 family, which measured shader types 4 and 11 only)
- **Description**: The importer's own stated rule is *"With model-space normals, slot 7 is the alternate specular intensity/colour texture rather than the normal backlight role."* That rule is implemented **once**, in the `_ =>` default arm. `BSLightingShaderType` 5 (SkinTint) and 6 (HairTint) are diverted into their own `5 | 6 =>` arm — added under #1350 to suppress a spurious slot-4/5 env-cube bind — and that arm reads **no slot at or above 3 at all**. Every model-space-normal SkinTint material therefore reaches `translate_material` with `MaterialTextureSet::specular == None` and `smooth_spec == None`. This is the **third member** of the same defect family as the two HIGHs closed hours earlier today (#2693 MultiLayerParallax slot 6, #2694 FaceTint slots 2/3): a shader-type arm intercepting a slot the generic path would have routed correctly.
- **Evidence**: Counted over the shipped Skyrim SE mesh archives with a throwaway `BSLightingShaderProperty` × `BSShaderTextureSet` cross-tabulator (measured, not estimated):

  ```
  Skyrim - Meshes0.bsa   shader_type=5: 1618 properties
                           model_space_normals:  803
                           slot 7 non-empty:     390
                           slot 7 AND msn:       390     (100 % overlap)
  Skyrim - Meshes1.bsa   shader_type=5:   13 properties
                           slot 7 AND msn:         4     (100 % overlap)
  ```

  Not one authored slot 7 on a *tangent*-space SkinTint material exists — the overlap with the MSN bit is exactly 100 % in both archives, i.e. this is precisely the population the `_ =>` arm's rule was written for. Confirmed structurally, not just by inference: `grep -rn "textures.get(" crates/nif/src/import/` returns exactly one slot-7 read in the whole importer (in `_ =>`), and `info.specular_map` has exactly one producer (the same line). Skyrim ships no BGSM, so `merge_external_material` cannot backfill it, and heads/bodies spawn through the loose-NIF path where no REFR `XTXR` overlay exists either. `shader_type == 6` (HairTint) measured **inert**: 10 815 properties, 0 MSN, 0 slot-7 occupancy — so the gap is SkinTint-only in practice and the fix is a one-arm change, not a re-plumb.
- **Impact**: Every Skyrim body, hands and beast-race skin material renders with a flat `specularStrength × specularColor` lobe instead of the authored per-pixel `_S.dds` mask. The consumer is live and shipping — `triangle.frag` multiplies `specColor` by the `specularMapIndex` sample when non-zero — so this is authored content the engine decodes, discards, and has a working sink for. No downstream fallback masks it: the normal-alpha-as-spec gloss binding would otherwise fire (it passes every other condition) but is gated on `normal_has_alpha`, and Skyrim's `_msn` maps are DXT1 — verified by extracting `textures\actors\character\female\femalebody_1_msn.dds` and reading its header (FourCC `DXT1`), which `format_has_alpha` (`crates/renderer/src/vulkan/dds.rs`) excludes. So the material lands with *no* specular mask rather than a wrong one — a cleaner failure, but a total loss of the authored signal.
- **Related**: #2693 / #2694 (the two sibling interception bugs fixed today); #2695 (the two disagreeing slot→role tables — a `slot_to_role(shader_type, slot)` helper would have made this arm's omission impossible to express).
- **Suggested Fix**: Hoist the `model_space_normals && info.specular_map.is_none()` slot-7 read out of the `_ =>` arm so it runs for every shader type (or explicitly add it to `5 | 6 =>`), and pin it with a fixture test asserting `shader_type == 5` + MSN + non-empty slot 7 → `MaterialInfo::specular_map`. Do **not** route it to `smooth_spec`: `_S.dds` is a specular *intensity/colour* map, while `smooth_spec` feeds `gloss_map_index`, which additionally suppresses the normal-alpha gate and modulates roughness — the exact conflation the default arm's comment and `byroredux/src/cell_loader/spawn/mesh_instance.rs`'s `effective_model_space_normals` branch both already warn against.

#### REN-D9-2026-08-12-01: `shrink_blas_scratch_to_fit`'s peak walk still ignores `skinned_blas` — the #2460 fix is not on `main`

- **Severity**: HIGH
- **Dimension**: 9 — Skinning / AS Correctness (shared BLAS build scratch)
- **Location**: `crates/renderer/src/vulkan/acceleration/memory.rs` — `AccelerationManager::shrink_blas_scratch_to_fit`; consumer `crates/renderer/src/vulkan/acceleration/blas_skinned.rs` — `AccelerationManager::refit_skinned_blas`
- **Status**: **Regression of #2460** — issue CLOSED 2026-08-08 with the comment "Fixed in `f3babea3`". See **Cluster C**.
- **Description**: `blas_scratch_buffer` is one allocation shared by the static BLAS builders (`build_blas`, `build_blas_batched`) and the skinned builders (`build_skinned_blas_batched_on_cmd`, `refit_skinned_blas`). At HEAD, `shrink_blas_scratch_to_fit` derives its shrink target from `self.blas_entries` only — `self.skinned_blas` is never consulted, exactly the shape #2460 described.
- **Evidence**:
  ```rust
  // crates/renderer/src/vulkan/acceleration/memory.rs, shrink_blas_scratch_to_fit
  let peak: vk::DeviceSize = self
      .blas_entries.iter().flatten()
      .map(|e| e.build_scratch_size).max().unwrap_or(0);
  ```
  Verified at merge: `git merge-base --is-ancestor f3babea3 HEAD` → **NOT-ancestor**; `git branch --no-merged main` lists `fix/2460-2461-2462-2463-as-rt-correctness`; `grep -rn "blas_scratch_peak" crates/renderer/src/` → **0 hits**, though `f3babea3` added that helper to `crates/renderer/src/vulkan/acceleration/predicates.rs`. `refit_skinned_blas` performs no scratch-size re-validation — it takes `self.blas_scratch_buffer.as_ref()`, reads the device address, aligns it, and submits `mode = UPDATE`.
- **Impact**: AS build-scratch overrun on any frame where a skinned entity survives a call to `shrink_blas_scratch_to_fit` — window resize (`crates/renderer/src/vulkan/context/resize.rs`) or cell unload (`byroredux/src/cell_loader/unload.rs`, where `unload_cell` only *queues* `pending_skin_unload_victims`, drained a later frame, so the outgoing cell's skinned BLAS are still resident). Consequences range from a corrupted neighbouring `gpu-allocator` slab entry to `VK_ERROR_DEVICE_LOST`. The `peak == 0` arm is the loud variant (scratch dropped entirely → every `refit_skinned_blas` fails its `.context(...)`); the shrink-to-static-peak arm is the silent one.
- **Related**: #2460 (closed-but-unmerged); #1782 (deferred scratch destroy — the *when*, orthogonal); #1127 (closed, stale premise). Collateral on the same branch: #2461, #2462, #2463.
- **Suggested Fix**: Merge `fix/2460-2461-2462-2463-as-rt-correctness` (or cherry-pick `f3babea3`), restoring `blas_scratch_peak` and its `crates/renderer/src/vulkan/acceleration/tests.rs` coverage so the union of `blas_entries` + `skinned_blas` drives both the `max()` walk and the `peak == 0` early-drop arm. Then re-verify the other three closures. Independently, the closure workflow needs a **"fix is reachable from `main`"** gate — this is a delivery-integrity failure, not a code-design one.

#### REN-D9-2026-08-12-02: the skin-compute descriptor cache treats a raw `vk::Buffer` handle as a stable identity, so a recycled global-vertex-SSBO handle produces a false cache hit

- **Severity**: HIGH
- **Dimension**: 9 — Skinning / Memory-Lifecycle
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs` — `SkinSlot::descriptor_bindings`, `SkinComputePipeline::dispatch`; interacts with `crates/renderer/src/mesh.rs` — `MeshRegistry::rebuild_geometry_ssbo`
- **Status**: NEW
- **Description**: `#1197 / PERF-DIM7-03` skips `vkUpdateDescriptorSets` when the live `(input_buffer, bone_buffer)` pair equals `slot.descriptor_bindings[frame_index]`. The key is a pair of raw non-dispatchable Vulkan handles compared for numeric equality. Vulkan's object model does **not** guarantee non-dispatchable handle values are unique or non-recycled — a handle freed by `vkDestroyBuffer` may be returned verbatim by the next `vkCreateBuffer`. The skinned path's `input_buffer` is the global vertex SSBO, which `rebuild_geometry_ssbo` reallocates on cell-stream growth; its `reclaim_before_rebuild` branch (#2374 low-headroom fallback) destroys the old buffer and allocates the replacement **inside the same call** — the maximum-probability recycle window. The deferred branch is reachable too, one rebuild later: a slot whose FIF descriptor was last written against handle `h1`, and which then skips dispatch across the `h2` generation (the `#1195` pose-dirty gate lets an idle NPC skip indefinitely), gets a false hit when a third rebuild recycles `h1`. Nothing invalidates `descriptor_bindings` externally — the struct doc explicitly relies on "the comparison handles any future rotation correctly without an explicit invalidation hook."
- **Evidence**: The renderer already treats this exact buffer as un-cacheable on the RT side. `crates/renderer/src/vulkan/context/draw.rs` re-points bindings 8/9 (`GlobalVertices`/`GlobalIndices`) **every frame, unconditionally**, with a comment recording the device loss that motivated it: *"…the binding was written only ONCE at scene setup … Without this per-frame refresh the descriptor keeps naming the OLD buffer, which `rebuild_geometry_ssbo` defers to the destroy queue … the next RT hit-fetch dereferences freed device memory → GPU page fault → ~TDR → `VK_ERROR_DEVICE_LOST`. (WATAL §0 device-loss hunt)"* `SkinComputePipeline::dispatch` binds the *same* buffer at set 0 binding 0 and guards it with `let needs_write = slot.descriptor_bindings[frame_index] != Some(live_key);`. `grep -rn "generation" crates/renderer/src/mesh.rs` returns no epoch counter to key against, and no site clears `SkinSlot::descriptor_bindings` outside `create_slot`. The buffer *size* is not part of the key either.
- **Impact**: A stale descriptor makes `skin_vertices.comp` read the freed generation's memory. The output is garbage skinned positions, which are then consumed by `build_skinned_blas_batched_on_cmd` / `refit_skinned_blas` as **AS build input** *and* dereferenced by `triangle.frag` / `water.frag` through `GpuInstance.skinnedVertexAddress`. Blast radius is every skinned actor's RT geometry for the affected slot, up to device loss.
- **Related**: #1197 (introduced the cache); #1782 / #2374 (the rebuild paths); the WATAL §0 device-loss precedent quoted above. `SkinPaletteComputePipeline`'s sibling cache is currently safe only because all three of its buffers are renderer-lifetime allocations.
- **Suggested Fix**: Add a monotonic `geometry_generation: u64` to `MeshRegistry`, bumped in `rebuild_geometry_ssbo`, and fold it into the cache key — or simply drop the compare-and-skip for binding 0 and keep it only for the palette buffer. **This is a CPU-side bookkeeping defect with a deterministic fix; it needs no barrier or stage change, so the no-speculative-Vulkan rule does not apply.** Unit-testable as a pure key-equality predicate, mirroring `should_evict_skin_slot` / `skin_slot_capacity_stale`. (Confirming the *symptom* on a specific driver would need `BYRO_VALIDATION=1`; confirming the *defect* does not.)

#### REN-D10-01: `cluster_cull.comp` derives per-tile ray directions from a difference of two ABSOLUTE positions — near-plane corners quantize onto the f32 grid and cluster frusta collapse in large worldspaces

- **Severity**: HIGH
- **Dimension**: 10 — Camera-Relative Precision
- **Location**: `crates/renderer/shaders/cluster_cull.comp` — `ndcToWorld`, and the `nearCorners` / `rayDir` / `corners` block in `main`
- **Status**: NEW. See **Cluster D-1**.
- **Description**: `ndcToWorld` correctly reconstructs from the render-origin-**relative** `invViewProj` and then lifts to ABSOLUTE (`+ renderOrigin.xyz`) so the cluster AABB shares the absolute space of the light SSBO — that part is right. The bug is *when* the lift happens: the very next use of those corners is a **small difference**, `normalize(nearCorners[i] - camPos)`, and both operands are now large-magnitude absolutes. The corners are unprojected at `ndcZ = 0.0` — the near plane, which `Camera::default()` puts at 0.1 world units (`crates/core/src/ecs/components/camera.rs`; standard non-reversed Z via `Mat4::perspective_rh`). So a ~0.1-unit vector is formed out of two ~10⁵-magnitude f32s. The relative corner (magnitude ≤ ~7 k, ULP ≈ 4.9e-4) is precise; the lift throws that precision away *before* it is used.
- **Evidence**: Verified at merge — `ndcToWorld` ends `return world.xyz / world.w + renderOrigin.xyz;` and line 177 is `vec3 rayDir = normalize(nearCorners[i] - camPos);`, with the four `nearCorners[i] = ndcToWorld(..., 0.0, invViewProj)` calls immediately above. At Markarth (`|world| ≈ 176 000`, f32 ULP = 0.015625) the render origin snaps to −176 128 and the whole near plane is 0.1473 units wide at the default 45° vertical FOV / 16:9 — 0.0092 units per tile across `CLUSTER_TILES_X = 16`, **below one ULP**. Reproducing the shader arithmetic in f32 for the 17 tile boundaries:

  ```
  tile  true x-offset   after +renderOrigin then −camPos    error
   0    -0.0736         -0.078125                          -0.0045
   1    -0.0644         -0.062500                          +0.0019
   2    -0.0552         -0.062500                          -0.0073   <-- == tile 1
   4    -0.0368         -0.031250                          +0.0056
   5    -0.0276         -0.031250                          -0.0036   <-- == tile 4
   6    -0.0184         -0.015625                          +0.0028
   7    -0.0092         -0.015625                          -0.0064   <-- == tile 6
  ```

  Adjacent boundaries collapse onto the same f32, so those tiles build a **zero-width frustum voxel** (and `inflate = aabbSize * 0.15` inflates zero by zero). Where they do not collapse, the residual is up to ~0.0073 on a 0.1-unit forward leg — a lateral direction error of ~4.5° against a tile's own angular size of ~5.3°. The error scales with the whole slice depth, since `corners[i] = camPos + rayDir * zFar` and `zFar` reaches `clusterFar()`.
- **Impact**: `sphereIntersectsAABB` tests real light spheres against wrong or degenerate AABBs, so `clusters[clusterIdx].count` under-reports, and `triangle.frag` iterates exactly that list — so affected tiles silently **lose point/spot lights**. Directional lights are unaffected (`lightType > 1.5` → `intersects = true` unconditionally), so the sun is safe and the symptom is exterior *local* lighting: lamps, campfires, neon and town torches dropping out in per-tile patches at night. Degradation is gradual and origin-magnitude-driven — ~10 % of a tile at `|world| ≈ 16 k`, ~42 % at 65 k, total collapse at ≥ 131 k. Interiors are unaffected, which is why this has not been noticed.
- **Related**: exactly the class of #1490 (`composite screen_to_world_dir` omitted the camera offset), #1642 (soft-particle depth fade mixed conventions) and #1488 (caustic re-projection). Distinct from #1092, which was about the *jittered* `inv_view_proj`, not the origin lift.
- **Suggested Fix**: Take the difference in relative space, then lift once. Keep an origin-relative `ndcToWorld` result, compute `vec3 camRel = cameraPos.xyz - renderOrigin.xyz;` (exact in f32 — the origin is a floor-multiple of `RENDER_ORIGIN_SNAP`) and `rayDir = normalize(nearCornerRel - camRel);` before building `corners[i] = camPos + rayDir * z` in absolute space, which the light test still needs. **Pure shader arithmetic reordering — no render pass, pipeline, barrier, or descriptor/UBO layout change.** Pin it with a static source-check test next to `caustic_writers_rebase_render_origin_before_reprojection`.

#### REN-D11-2026-08-12-01: refractive glass is the only `INSTANCE_FLAG_CAUSTIC_SOURCE` population, and the same predicate now masks its mesh-ID write off — `caustic_splat.comp` can never find a source pixel

- **Severity**: HIGH
- **Dimension**: 11 — Pipeline/RenderPass (G-buffer attachment write masks)
- **Location**: `crates/renderer/src/vulkan/pipeline.rs` (`create_blend_pipeline`, the `preserve_opaque_gbuffer` branch of `attachments`); `crates/renderer/src/vulkan/context/draw.rs` (`is_refractive_glass` / `is_caustic_source`, and the `PipelineKey::Blended` construction that sets `preserve_opaque_gbuffer: order_dependent_glass`); `crates/renderer/shaders/caustic_splat.comp` (the `meshIdTex` gate)
- **Status**: NEW — introduced by `c615f8de` (2026-08-11), one day before this audit. No open or closed issue matches. See **Cluster B**.
- **Description**: `c615f8de` added the `preserve_opaque_gbuffer` axis to `PipelineKey::Blended` and, when set, replaced attachments 1–5 (`normal`, `motion`, **`mesh_id`**, `raw_indirect`, `albedo`) with `no_write`. The flag is set from `is_refractive_glass(draw_cmd)`; `INSTANCE_FLAG_CAUSTIC_SOURCE` is set from `is_caustic_source(draw_cmd)`, which is literally `is_refractive_glass(cmd)` — the same predicate, deliberately kept as one function. `caustic_splat.comp` finds its sources exclusively through the mesh-ID attachment. Both arms are now unreachable for glass: glass **with** `alpha_blend` has its `outMeshID` discarded by the write mask, leaving the opaque receiver's ID with bit 31 clear; glass **without** `alpha_blend` (the MultiLayerParallax arm) takes the opaque pipeline, so `alphaBlendFrag` is false and bit 31 is never set. Every other bit-31 pixel comes from a non-glass blended draw (particles, FX cards, decals), which by construction fails the `INSTANCE_FLAG_CAUSTIC_SOURCE` gate. **The producing and consuming sets are disjoint.**
- **Evidence**: Verified at merge:
  ```rust
  // crates/renderer/src/vulkan/context/draw.rs
  fn is_caustic_source(cmd: &DrawCommand) -> bool { is_refractive_glass(cmd) }
  let order_dependent_glass = is_refractive_glass(draw_cmd);
  PipelineKey::Blended { …, preserve_opaque_gbuffer: order_dependent_glass }
  if is_caustic_source(draw_cmd) { f |= INSTANCE_FLAG_CAUSTIC_SOURCE; }
  ```
  ```
  crates/renderer/src/vulkan/pipeline.rs:654   let attachments = if preserve_opaque_gbuffer {
                                                   [hdr_blend, no_write, no_write, no_write /* 3 mesh_id */, …]
  crates/renderer/shaders/caustic_splat.comp   if ((meshIdRaw & 0x80000000u) == 0u) return;
                                               uint flags = instances[instIdx].flags;  // CAUSTIC_SOURCE gate
  ```
  The intent contradiction survives in three live places that were not updated: `triangle.frag`'s `stableSurfaceId` block (*"The caustic compute pass consumes that index…"*), `crates/renderer/src/vulkan/gbuffer.rs`'s `MESH_ID_FORMAT` doc (*"Stable surface ID / alpha draw lookup"*), and `docs/engine/shader-pipeline.md`'s Mesh-ID note.
- **Impact**: The glass-side caustic accumulator (#321 / M22 Option A, live since `91638ec4`, 2026-04-16) receives **zero splats on every frame in every cell**, while the compute pass still dispatches and still pays its full screen-sized cost. Water-side caustics are unaffected. The commit's own rationale ("caustics through walls") suggests the mask was meant to *fix* a caustic leak, so this may be an over-broad fix rather than an unnoticed one — but as written the feature is off, not corrected, and the mesh-ID contract in three docs no longer describes the code. **Compounds with REN-D14-NEW-01: fixing either alone leaves the pass dark.**
- **Related**: `883f57cd` (the mesh-ID bit-meaning split — Cluster A), #321, #992, #2468, REN-D14-NEW-01, REN-D21-01 (why the Cornell harness cannot bisect this).
- **Suggested Fix**: Decide which contract survives and make it single-sourced — either (a) keep `mesh_id` writable for the glass pipeline (split the write-mask set so attachment 3 stays `overwrite` while 1/2/4/5 stay `no_write`) and solve "caustics through walls" in `caustic_splat.comp`'s depth/geometry gate, or (b) retire the alpha-draw mesh-ID representation and give `caustic_splat.comp` an explicit source list — then update `triangle.frag`, `gbuffer.rs::MESH_ID_FORMAT` and `docs/engine/shader-pipeline.md` together. A unit test asserting `is_caustic_source(cmd) ⇒ mesh-ID is writable for that cmd's pipeline key` would have caught this at `cargo test` time.

---

### MEDIUM (29)

#### REN-D1-01: `docs/engine/renderer.md`'s AS section contradicts the code on three points, including the #516 "off-screen occluders stay in the TLAS" invariant

- **Severity**: MEDIUM
- **Dimension**: 1 — AS Correctness (code/doc divergence; the **doc** is the wrong side)
- **Location**: `docs/engine/renderer.md` — the "TLAS per frame", "Per-skinned-entity BLAS (M29)" and instance-staging bullets. Code sides: `crates/renderer/src/vulkan/acceleration/tlas.rs`, `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`.
- **Status**: NEW
- **Description**: Three statements describe behaviour the code deliberately does *not* have. (1) *"TLAS per frame: rebuilt every frame … **with frustum culling against the camera**"* — the code says the opposite and says so as a fixed bug: the only TLAS-eligibility gate is `draw_command_eligible_for_tlas` (`crates/renderer/src/vulkan/acceleration/predicates.rs`), which reads `in_tlas && !is_water` with no frustum term. (2) *"Per-skinned-entity BLAS (M29): keyed by `EntityId`, built sync at cell load"* — the synchronous per-NPC builder was deleted under #1141; `build_skinned_blas_batched_on_cmd` is the sole entry point and records onto the per-frame command buffer at first sight. (3) *"a two-stage barrier chain (HOST_WRITE→TRANSFER_READ→**AS_READ**)"* — the second barrier's `dst_access_mask` is `SHADER_READ`, which is precisely what #1436 changed.
- **Evidence**: `build_tlas_instances` carries the #516 rationale verbatim (*"off-screen occluders stay in the TLAS so on-screen fragments' shadow / reflection / GI rays hit them"*); `blas_skinned.rs`'s module doc names the batched builder the *"Sole entry point"*; `tlas.rs`'s `build_tlas` uses `.dst_access_mask(vk::AccessFlags::SHADER_READ)` with an in-code comment explaining that `ACCELERATION_STRUCTURE_READ_KHR` is what sync-validation flags as a copy→build RAW hazard. `docs/engine/memory-budget.md`'s AS section was checked against the same code and is **accurate** — the drift is confined to `renderer.md`.
- **Impact**: Doc-driven regressions. Item 1 is the dangerous one: it reads as a design statement, it sits in the doc `_audit-common.md` points readers at for BLAS/TLAS lifecycle, and acting on it silently deletes RT contributions from every off-screen occluder. Item 3 would re-trip the sync-validation hazard #1436 fixed.
- **Related**: #516, #1141, #1436.
- **Suggested Fix**: Three edits to `docs/engine/renderer.md` — drop "with frustum culling against the camera" and state the #516 rule (frustum gates `in_raster` only); change "built sync at cell load" to "built on the per-frame command buffer at first sight"; change the barrier chain to `HOST_WRITE→TRANSFER_READ→SHADER_READ @ AS_BUILD` and cite #1436.

#### REN-D3-2026-08-12-01: `GpuInstance`'s five-mirror lockstep guard is presence-only — the weakest of the four hand-mirrored GPU structs, and its own protocol comment describes a mechanism the test does not implement

- **Severity**: MEDIUM
- **Dimension**: 3 — GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` (`every_shader_struct_gpu_instance_names_material_kind_slot`); `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuInstance` doc comment)
- **Status**: NEW
- **Description**: `GpuInstance` has the largest hand-mirror fan-out of any GPU struct — 5 declaration sites (`crates/renderer/shaders/include/bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp`) — and the documented recurring trap is precisely a mirror reading wrong offsets (#785 / #1498). Yet its only cross-mirror guard is a `src.contains()` needle check: it asserts each file declares `struct GpuInstance`, contains three field-name strings, does not contain `"uint _pad1"`, and does not re-introduce 26 retired names. It never compares the mirrors to each other, never compares them to the Rust struct, and never checks field **order** or **completeness**. Compounding it, the `gpu_types.rs` doc instructs contributors to "update the expected suffix in the assertion and rename the sentinel" — there is no expected-suffix logic and no sentinel in the test. The test's own name refers to a field that was removed from the struct.
- **Evidence**: Fields with **no** coverage in any mirror: `model`, `textureIndex`, `boneOffset`, `vertexOffset`, `indexOffset`, `vertexCount`, `flags`, `ior`, `avgAlbedoR/G/B`, `_reserved`. Worked failure the guard cannot see: delete `float ior;` from `ui.vert`'s mirror — `skinnedVertexAddress` re-aligns to 8, **stride stays 128 B**, so no stride check catches it, while `surfaceId` now reads `avg_albedo_b`'s bytes, and every assertion still passes. Its two siblings already have the guard it lacks, in the *same file*: `gpu_light_glsl_copies_stay_in_lockstep` (#1916) does full stripped-body equality; `gpu_material_glsl_field_order_matches_rust_struct` (#1657) does a full ordered Rust↔GLSL comparison. CI does not backstop it — `scripts/check-shader-artifacts.sh` proves each `.spv` is reproducible from its GLSL, which says nothing about whether that GLSL matches the Rust struct; SSBO `ArrayStride` has no reflection helper at all.
- **Impact**: A one-line edit to any of the 4 standalone mirrors can silently desync per-instance reads while `cargo test` and the CI shader job both stay green. Blast radius by mirror: `triangle.vert` = every drawn vertex; `water.vert` = every water plane; `caustic_splat.comp` = every caustic deposit; `ui.vert` = the UI overlay. **No drift exists today** — all 5 mirrors and all 6 `.spv` were verified byte-identical this run.
- **Related**: #1916 (the pattern to copy), #1657, #2463 (`GpuTerrainTile`, same class one rung lower — Cluster C), #785 / #1498 (the historical incidents), #2164 (`_pad_id0` → `ior`), #2433.
- **Suggested Fix**: Add `gpu_instance_glsl_copies_stay_in_lockstep`, modelled directly on `gpu_light_glsl_copies_stay_in_lockstep`: `extract_struct_body` + `strip_struct_body` across all 5 sites, assert byte-identical stripped field lists, then reuse `parse_rust_struct_fields` / `normalize_ident` to assert the shared list matches the Rust field order. Fix the `gpu_types.rs` protocol comment to describe the real mechanism and rename the test off `material_kind`.

#### REN-D3-2026-08-12-02: `GpuCamera::dof_params.zw` carries live data but is documented "reserved (0)" in the Rust struct and 4 of the 5 GLSL mirrors

- **Severity**: MEDIUM
- **Dimension**: 3 — GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera::dof_params`); `crates/renderer/shaders/triangle.vert`, `water.vert`, `cluster_cull.comp`, `caustic_splat.comp` (their `CameraUBO` `dofParams` declarations); writer `crates/renderer/src/vulkan/context/draw.rs`; readers `crates/renderer/shaders/include/lighting.glsl`, `crates/renderer/shaders/triangle.frag`
- **Status**: NEW (a 2026-07-09 audit fixed the *same* rot in `docs/engine/shader-pipeline.md`; the in-code sites were never corrected)
- **Description**: The Rust doc reads *"x = aperture half-radius, y = focal distance, **zw = reserved (0)** … Available to shaders for future screen-space DOF … without an extra UBO binding."* Both lanes are live: `.z` = `light_atten_knee` (the #1451 point/spot attenuation knee, live-tunable via the `light.atten` console command), `.w` = `camera_static`. Byte layout is correct everywhere — this is a **semantic** lockstep break, and the wording is not merely stale but an active invitation to repurpose a lane that has two consumers.
- **Evidence**: Writer: `dof_params: [active_dof.aperture, active_dof.focus_dist, self.light_atten_knee, if camera_static { 1.0 } else { 0.0 }]`. Readers: `crates/renderer/shaders/include/lighting.glsl` — `float kneeFrac = (dofParams.z > 0.0001) ? dofParams.z : 0.5;`; `triangle.frag` — `bool cameraStatic = dofParams.w > 0.5;` and `float giSeed = dofParams.w > 0.5 ? frameCount : floor(frameCount * 0.25);`. Declaration-site audit: `crates/renderer/shaders/include/bindings.glsl` is **correct**; `triangle.vert`, `water.vert`, `cluster_cull.comp`, `caustic_splat.comp` and `gpu_types.rs` all say `zw = reserved`. The authoritative `docs/engine/shader-pipeline.md` is correct, so this is code-side divergence from an already-fixed doc.
- **Impact**: No runtime effect today. The failure mode is a future author trusting the Rust struct doc or 4 of 5 GLSL mirrors, treating `.z`/`.w` as free, and silently breaking point/spot attenuation shaping plus the parked-camera GI-seed decorrelation that makes indirect lighting converge. The exact trap the codebase already burned on with `_pad_id0` → `ior`.
- **Related**: #2164, #1928 (`VolumetricsParams::render_origin.w` overload — same class), #1451, #2483 / #2433 / #2415.
- **Suggested Fix**: Replace `zw = reserved (0)` in `gpu_types.rs` with the `crates/renderer/shaders/include/bindings.glsl` wording plus the consumer list, and propagate the same one-line comment to the four standalone `CameraUBO` mirrors. Comment-only — no `.spv` recompile, so `scripts/check-shader-artifacts.sh` is unaffected.

#### REN-D4-05: the image-health readback buffer is allocated with the upload-only `MemoryLocation::CpuToGpu`, while the repo's only other readback correctly uses `GpuToCpu`

- **Severity**: MEDIUM
- **Dimension**: 4 — Sync/Barriers (host-access classification; borders Memory/Lifecycle)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs` (the `image_health_buffers` construction loop in `VulkanContext::new`, via `GpuBuffer::create_host_visible`); `crates/renderer/src/vulkan/buffer.rs` (`create_host_visible` hard-codes `MemoryLocation::CpuToGpu`). Contrast: `crates/renderer/src/vulkan/context/screenshot.rs` — `ensure_screenshot_staging` (`GpuToCpu`).
- **Status**: NEW
- **Description**: The counter buffers are created through `GpuBuffer::create_host_visible`, whose allocation site pins `location: MemoryLocation::CpuToGpu` with no parameter to vary it. That location exists for staging *uploads*; gpu-allocator resolves it toward `HOST_VISIBLE | HOST_COHERENT` and on a discrete card frequently device-local BAR memory, i.e. uncached write-combined from the CPU's point of view. `GpuToCpu` is the readback location and additionally prefers `HOST_CACHED`. The image-health buffer is read by the host **every frame, on the hot path, right after the fence wait** — it is a readback buffer wearing an upload buffer's allocation.
- **Evidence**: `create_host_visible`'s `AllocationCreateDesc` hard-codes `location: MemoryLocation::CpuToGpu` with `name: "host_visible_buffer"`. `grep -rn "MemoryLocation::GpuToCpu" crates/renderer/src` returns exactly one site — the screenshot staging buffer. `IMAGE_HEALTH_BUFFER_BYTES = 8`, far below a typical `nonCoherentAtomSize` of 64, so any future flush/invalidate has to be atom-aligned and the aligned range can reach into neighbouring suballocations — a problem `GpuBuffer::flush_range` already grapples with on the write side.
- **Impact**: Mis-tiered allocation on a per-frame host-read path, plus a coupling that makes the correct-looking fix dangerous in isolation: `GpuToCpu` steering toward `HOST_CACHED` is precisely the case where REN-D4-04's missing invalidate becomes observable. No visual defect.
- **Related**: REN-D4-04 (the availability half — **must move together**), REN-D5-05 (the documentation half), #2736.
- **Suggested Fix**: Observation only for now. If acted on, the two findings must move together — switching to `GpuToCpu` first requires REN-D4-04's invalidate/availability step to exist, or the change trades a theoretical staleness risk for a likelier one.

#### REN-D5-02 / REN-D16-02: `froxel_xy_divisor` default moved 12 → 8 without propagating — `memory-budget.md`, two in-code doc blocks and one design doc still quote the 12-derived grid (VRAM understated 2.25×)

- **Severity**: MEDIUM
- **Dimension**: 5 (Memory/Lifecycle) **and** 16 (Volumetrics) — **found independently by both; the concurrence strengthens it**
- **Location**: Ground truth `crates/renderer/src/vulkan/upscaling.rs` (`impl Default for VolumetricsConfig`, `froxel_xy_divisor: 8`), consumed by `crates/renderer/src/vulkan/volumetrics.rs` (`froxel_extent`). Stale sites: `docs/engine/memory-budget.md` (§ "Volumetrics (M55)" + the *Volumetrics froxel grid* row of "VRAM Rough Budget"); `crates/renderer/src/vulkan/volumetrics.rs` (`VOLUMETRIC_OUTPUT_CONSUMED` doc block); `crates/renderer/shaders/volumetrics_inject.comp` (header); `crates/renderer/shaders/volumetrics_integrate.comp` (header); `docs/engine/procedural-volumetric-fog.md`.
- **Status**: **Regression of #2230** (CLOSED by `583e0ae7` — the commit that wrote the now-stale `/12` formula). Carried from Dim 16's classification.
- **Description**: The doc states *"One froxel column per `froxel_xy_divisor` (default 12) render pixels in X/Y"* and derives its whole table from `ceil(width/12) × ceil(height/12) × 64 × 8 B × 2 volumes × 2 FIF`. The live default is **8**, changed by `5798e467` (2026-08-09), which is the only commit that ever touched the literal. The structural invariant is fine — `froxel_extent(render_extent, config)` really is derived from the render extent and really is downstream of the FSR preset query — it is the divisor it keys on that drifted. Since the footprint scales with the square of the divisor, the error is exactly `(12/8)² = 2.25×`.

  | Site | Documented | Live |
  |---|---|---|
  | grid @ 1080p render extent | 160×90×64 | **240×135×64** |
  | total @ 1080p | ~29.5 MB | **~66.4 MB** |
  | total @ 1440p | ~52.6 MB | **~118.0 MB** |
  | total @ 4K | ~118.0 MB | **~265.4 MB** |
  | inject ray budget | ~9.2 M queries/frame | **~20.7 M** |
  | integrate columns | 14 400 | **32 400** |

  Cross-check on the drift: the doc's stated *4K* figure (118 MB) is now exactly the live *1440p* figure.
- **Evidence**: Confirmed at merge — `crates/renderer/src/vulkan/upscaling.rs:115` reads `froxel_xy_divisor: 8`, while `docs/engine/memory-budget.md:177` still says *"(default 12)"* and `:181` still carries `ceil(width / 12) × ceil(height / 12)`. `git log -S "froxel_xy_divisor: 8"` → `5798e467` only; `git log -S"ceil(width / 12)" -- docs/engine/memory-budget.md` → exactly `583e0ae7`, the #2230 fix. `5798e467` *did* add `default_volumetrics_resolve_one_froxel_per_eight_pixels` pinning the constant — so the constant has a test and the budget doc has no lockstep guard. Two smaller unledgered sites in the same section: the density-noise 3D volumes (`crates/renderer/src/vulkan/volumetrics/noise.rs`, ~262 KB + ~32 KB, the pass's only *non*-resolution-scaled allocations) and the per-FIF `dalc_buffers`.
- **Impact**: `memory-budget.md` is the audit-designated authority for VRAM ceilings against a 6 GB RT minimum and a "< 4 GB total" design target. It under-reports the volumetric grid by 37 MB at 1080p and 147 MB at 4K. Combined with the still-open #2679, the "VRAM Rough Budget" total is understated by roughly 100 MB typical / 400 MB peak. **No runtime defect** — the allocation itself is correct, FIF-correct and leak-free. #2509's per-froxel ray-count analysis also reasoned from the stale 9.2 M figure.
- **Related**: #2230 (regressed), #2509 (inherits the stale grid), #2679 (sibling ledger drift), #1814 (the self-reporting-allocation mechanism), #1872, #2038, #2314.
- **Suggested Fix**: Recompute the memory-budget table and summary row from `VolumetricsConfig::default().froxel_xy_divisor`, fix the four in-tree doc sites, add the noise volumes and `dalc_buffers` as ledger rows, and add a unit test tying the documented 1080p figure to `froxel_extent(1920×1080, VolumetricsConfig::default())` so the next divisor change fails loudly. Add the #1814 one-line `log::info!` size report at `VolumetricsPipeline::new` — a self-reporting allocation is the only thing that has actually stopped this drift recurring.

#### REN-D5-03: two distant-LOD spawn paths leak GPU handles on their `entities.is_empty()` early return — the exact `#1537` shape the `.btr` sibling guards against explicitly

- **Severity**: MEDIUM
- **Dimension**: 5 — Memory/Lifecycle
- **Location**: `byroredux/src/cell_loader/object_lod.rs` (`spawn_object_lod_quad`) and `byroredux/src/cell_loader/placement_lod.rs` (`spawn_placement_lod_cell`). Correct sibling: `byroredux/src/cell_loader/terrain_lod_btr.rs` (the upload-failure arm).
- **Status**: NEW
- **Description**: Both functions acquire refcounted GPU resources into locals, then bail through an `entities.is_empty()` guard that discards the locals without releasing them. `World::despawn` has no GPU side effects and the returned block is `None`, so nothing downstream can perform the matching release — `unload_object_lod_block` / `unload_placement_lod_block` never see these handles. `spawn_object_lod_quad` resolves the worldspace object atlas **once, before** the per-sub-mesh loop, and every sub-mesh can then be skipped (empty positions/indices) or fail its upload. `spawn_placement_lod_cell` is worse: `mesh_handles` and `texture_handles` are declared **before** the group loop and accumulate uploaded global-SSBO mesh ranges and texture refcounts across *all* groups, so a `.lod` whose groups all carry `count == 0` (the parser accepts it) strands every one of them.
- **Evidence**: `object_lod.rs` — `let atlas = resolve_texture(ctx, tex_provider, Some(atlas_path.as_str()));` sits above the `for mesh in &imported.meshes` loop, and the only `drop_texture` for it is in `unload_object_lod_block`, reachable only via a returned `ObjectLodBlock`. `placement_lod.rs` — the three `Vec::new()` declarations precede the group loop; the pushes run inside it; the `entities.is_empty()` return follows with no release. `terrain_lod_btr.rs` proves the contract is understood at this layer: its upload-failure arm calls `drop_texture` on both handles with the comment *"Release the refs the two resolves above acquired, or a failed upload pins their VkImages + bindless slots for the session (the #1537 leak shape)"*.
- **Impact**: Stranded bindless texture slots and stranded global vertex/index SSBO ranges that no reclaim path can ever free. Slot-space exhaustion is the documented slow-motion failure mode for `TextureRegistry` (#2030 — grow-only slot space makes each stranded slot permanent); the mesh side pins pool bytes against `VERTEX_POOL_SOFT_CAP` / `INDEX_POOL_SOFT_CAP`. The object-LOD case is a *stuck refcount* rather than runaway growth (every quad in a worldspace resolves the same atlas path). Reachability is genuinely narrow — a `.bto` whose every sub-mesh is degenerate or whose uploads all fail under memory pressure; a `.lod` with only zero-count groups — hence MEDIUM rather than the HIGH floor.
- **Related**: #1537 (the original LOD texture-refcount leak), #2030 / MEM-D3-01, #2374 / EX-08 (the exterior resource-ownership soak — the natural place to assert this).
- **Suggested Fix**: Release before each early return, mirroring `terrain_lod_btr.rs`: in `spawn_object_lod_quad` drop `atlas` (when non-zero and not the fallback) on the `entities.is_empty()` path; in `spawn_placement_lod_cell` drain `mesh_handles` through `drop_mesh` and `texture_handles` through `drop_texture` on the same path. Cheapest durable guard: extend the #2374 ownership soak to assert `live_static_blas_count` / `live_slot_count` return to baseline after a LOD block that spawns nothing.

#### REN-D8-NEW-01: both SVGF passes mask `ALPHA_BLEND_NO_HISTORY` off before comparing mesh IDs, so an opaque pixel can be matched against an alpha-blended fragment's *draw index*

- **Severity**: MEDIUM
- **Dimension**: 8 — Denoiser/Composite. See **Cluster A**.
- **Location**: `crates/renderer/shaders/svgf_temporal.comp` — the bilinear-tap predicate `if ((prevID & 0x7FFFFFFFu) != (currID & 0x7FFFFFFFu)) continue;` and the sub-pixel-motion fallback; `crates/renderer/shaders/svgf_atrous.comp` — the spatial tap rejection `if ((idQ & 0x7FFFFFFFu) != (idP & 0x7FFFFFFFu)) continue;`
- **Status**: NEW (related to CLOSED #904, #1159, #992 — the masking those added was correct when both halves of the encoding meant the same thing; `883f57cd` changed the opaque half's meaning and neither predicate was revisited)
- **Description**: Bits 0–30 carry two namespaces (ECS entity index for opaque, per-frame sorted draw index for alpha-blended), and bit 31 is the only discriminator. Both predicates mask it away and compare the low 31 bits. Both namespaces are small dense counters from overlapping ranges, so this is a **systematic aliasing condition**, not a wide-hash collision: whichever (entity id, draw index) pair coincides keeps coinciding frame after frame. The two consumers behave very differently, and the distinction is load-bearing for the severity. `svgf_temporal.comp` is currently self-limiting **by accident** — an alpha-blended pixel takes the early-out and writes history age 0, and `prevMeshIdTex` / `prevMomentsHistTex` bind the same `prev` slot, so any colliding tap carries `histAge == 0` → `invN = 1.0` → `alphaC = 1.0` → the no-history result exactly. The residue is the *mixed* bilinear case, where a colliding tap dilutes `histAge` and injects a foreign pixel's indirect at its bilinear weight. `svgf_atrous.comp` has **no such bound**: mesh-ID rejection is the only identity gate in its tap loop, and a colliding neighbour contributes at full weight.
- **Evidence**: `triangle.frag` — `uint meshIdBase = alphaBlendFrag ? sortedInstanceId : stableSurfaceId;` / `outMeshID = meshIdBase | (alphaBlendFrag ? 0x80000000u : 0u);`. `crates/renderer/src/vulkan/context/draw.rs` — `surface_id: draw_cmd.entity_id.wrapping_add(1)`. `crates/renderer/shaders/triangle.vert` — `fragInstanceIndex = gl_InstanceIndex;`. `crates/renderer/src/vulkan/pipeline.rs` — the non-`preserve_opaque_gbuffer` blend attachment array marks slot 3 (mesh_id) and slot 1 (normal) `overwrite`, so **every** particle, smoke, decal, fade and BSEffect draw overwrites the opaque mesh ID. Two of the à-trous filter's four guides are weak in exactly this situation: the same branch overwrites the *normal* attachment (so a camera-facing billboard passes `pow(dot, 128)` against a camera-facing wall), and alpha-blended draws never write depth (so `wZ` compares the receiver's depth against itself, ≈ 1). `crates/renderer/src/vulkan/svgf.rs::write_descriptor_sets` binds `mesh_id_views[prev]` and `moments_history[prev].view` from the same index — the basis for the `histAge == 0` argument.
- **Impact**: Spatial leak of a transparent fragment's demodulated indirect into an unrelated opaque surface's à-trous filter, at up to a 14-render-pixel radius (`ATROUS_ITERATIONS = 3`), localized to colliding pairs and therefore **stable frame-to-frame rather than noise-like**. Secondary: accelerated temporal history decay at particle silhouettes. Visual only — no GPU or memory hazard. Affects every scene with alpha-blended content, all games.
- **Related**: `883f57cd`, #904, #1159, #992, #2116 (the sibling namespace bug already fixed on the caustic side), #2160 (the same collision class fixed on the CPU rigid motion-history map), REN-D13-01 (the identical predicate in `taa.comp` — fix both together).
- **Suggested Fix**: In all three predicates, treat *bit 31 set on the other sample* as an outright non-match instead of masking it. This cannot regress #904/#1159's motivating case: refractive glass now takes the `preserve_opaque_gbuffer` path and does not write mesh ID at all, so "a single instance toggling between opaque and blended" is no longer representable. A cheap source-order guard test in `crates/renderer/src/vulkan/svgf.rs` (same shape as `svgf_atrous_stops_on_depth_and_albedo_edges`) would pin the predicate.

#### REN-D10-02: `getHitTriWorldPositions` returns RELATIVE positions on the rigid branch and ABSOLUTE on the skinned branch, under a name and doc that both claim absolute world space

- **Severity**: MEDIUM
- **Dimension**: 10 — Camera-Relative Precision. See **Cluster D**.
- **Location**: `crates/renderer/shaders/include/ray_hit.glsl` (`getHitTriWorldPositions`, consumed by `getHitTriNormal` and `getRayHitTangentFrame`)
- **Status**: NEW
- **Description**: The two branches emit positions in two different conventions. Skinned (`hi.boneOffset != 0 && hi.skinnedVertexAddress != 0ul`) reads the `skin_vertices.comp` output, which bakes the **absolute** bone palette — the same convention `tlas_instance_transform` relies on when it emits `IDENTITY_VK_TRANSFORM` for skinned instances. Rigid multiplies bind-pose vertices by `hi.model`, and `GpuInstance.model` has been **render-origin-relative** since the markarth cascade — `rebase_model_matrix` subtracts `render_origin` from the translation column of every draw, unconditionally. The header comment reads *"World-space positions of a ray-query hit triangle's three vertices"* and the #2219 block states the skinned positions are *"already absolute-world"*, which reads as a whole-function guarantee it does not hold.
- **Evidence**: `crates/renderer/src/vulkan/context/draw.rs` — `let current_model = rebase_model_matrix(m, render_origin);` runs for every `draw_cmd` before the `GpuInstance` is pushed, while `tlas_instance_transform(draw_cmd)` consumes the un-rebased `draw_cmd.model_matrix`. `ray_hit.glsl` then does `w0 = (hi.model * vec4(v0, 1.0)).xyz;` in one branch and a raw `SkinnedVertexRef` read in the other.
- **Impact**: **No wrong pixels today** — every consumer uses only differences (`normalize(cross(w1 - w0, w2 - w0))` in `getHitTriNormal`; `edge1`/`edge2` in the `getRayHitTangentFrame` UV-gradient fallback), and a uniform origin offset cancels in both. The exposure is that a public-looking helper whose name, header and doc all promise absolute world space hands the next caller a silently branch-dependent frame: any re-projection, distance-to-camera, world-space hash, or second-bounce origin gets a `render_origin`-sized displacement on rigid geometry only. That is the same failure #1488 shipped for the caustic writers. Reported at MEDIUM rather than the dimension's HIGH floor because the mixing is latent, not live — **escalate on the first absolute consumer**.
- **Related**: #2219 (added the skinned branch), #1487 (skinned TLAS identity), #1488.
- **Suggested Fix**: Make the contract explicit rather than changing behaviour — either rebase the skinned branch to relative (`-= renderOrigin.xyz`) so both branches return relative and rename to `getHitTriPositionsRel`, or lift the rigid branch to absolute and keep the current name. Either way state in the header which frame is returned; the difference-only consumers are unaffected by the choice.

#### REN-D11-2026-08-12-02: four production early-returns in `triangle.frag` fall through to the `outFsrReactive/Transparency = 0.0` defaults that were written for the debug and sky arms

- **Severity**: MEDIUM
- **Dimension**: 11 — Pipeline/RenderPass (G-buffer attachment 6/7 write completeness)
- **Location**: `crates/renderer/shaders/triangle.frag` — the mask initialisation at the top of `main()`, the four early `return`s (the `MATERIAL_KIND_EFFECT_SHADER` additive arm, the `MATERIAL_KIND_NO_LIGHTING` arm, the IOR/RT glass arm, and the `DBG_VIZ_GLASS_PASSTHRU` arm), and the tail policy
- **Status**: NEW (#2518 is a different FSR finding — DOF gating)
- **Description**: `main()` opens by zeroing both FSR masks, with a comment scoping the default to *"the debug visualizations, the sky/background arms"*. Four arms that return early are neither — they are production transparent-surface paths. The RT/IOR glass arm is the strongest case: the tail policy explicitly says `isGlass → outFsrTransparency = 1.0` (*"Refractive glass is the clear case: the pixel's colour tracks what is behind it through an IOR bend, so its motion vector describes the wrong surface entirely"*), yet glass that *takes* the IOR branch returns before that line and reports 0.0, while the same glass falling back to the Fresnel path (LOD ≥ `RT_LOD_IOR`, or ray budget exhausted) reaches the tail and reports 1.0. The `FIRE_REFRACTION` arm shows the intended discipline — it sets both masks to 1.0 before its `return` — which makes the other three look like omissions rather than policy.
- **Evidence**: The `EFFECT_SHADER`, `NO_LIGHTING` and IOR-glass arms each end `outColor = …; … return;` with no mask write, against `FIRE_REFRACTION`'s `outFsrReactive = 1.0; outFsrTransparency = 1.0; return;` and the tail `outFsrTransparency = isGlass ? 1.0 : (isAlphaBlend ? fsrCoverage : 0.0);`.
- **Impact**: FSR 3.1 gets no reactive / transparency-and-composition hint for refractive glass on the RT path, additive `BSEffectShaderProperty` FX cards, and `BSShaderNoLightingProperty` surfaces — most of the content class the masks exist for. Because both masks MAX-blend, writing 0.0 never corrupts another draw, so the symptom is one-sided: history is kept where it should be rejected (smearing/ghosting behind flames, glow cards, terminal screens and RT glass), and the same glass object can **flip its transparency mask between 1.0 and 0.0 frame-to-frame** as the adaptive ray budget moves it across `RT_LOD_IOR`. On the engine's default render path.
- **Related**: REN-D11-2026-08-12-01 (same commit-era pipeline/G-buffer surface), #2518, Dim 23.
- **Suggested Fix**: Hoist the tail policy into a small helper called immediately before each production early `return` (or compute `fsrCoverage` early and set both masks once, before the branch ladder), and narrow the top-of-`main` default's comment to what it actually covers.

#### REN-D12-2026-08-12-01: the indirect draw loop is not clamped to `MAX_INDIRECT_DRAWS`, so a batch overflow records a `cmd_draw_indexed_indirect` that reads past the end of the indirect buffer

- **Severity**: MEDIUM
- **Dimension**: 12 — Command buffer recording
- **Location**: `crates/renderer/src/vulkan/context/geometry_pass.rs` (`record_geometry_pass`, the `while i < batches.len()` draw loop) vs. `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`upload_indirect_draws`)
- **Status**: NEW
- **Description**: `upload_indirect_draws` clamps its write to `draws.len().min(MAX_INDIRECT_DRAWS)` and logs a one-shot warn on overflow — the same "log and continue" policy `upload_instances` uses for `MAX_INSTANCES` (#647 / RP-1). The **consumer has no matching clamp**: the draw loop walks the full `batches` slice and computes `byte_offset = i * indirect_stride` with `group_size = end - i`, so when `batches.len() > MAX_INDIRECT_DRAWS` the recorded call names a range that starts or ends beyond the buffer's allocation. `indirect_buffers[frame]` is sized exactly `size_of::<VkDrawIndexedIndirectCommand>() * MAX_INDIRECT_DRAWS` (`crates/renderer/src/vulkan/scene_buffer/buffers.rs`), and `MAX_INDIRECT_DRAWS == MAX_INSTANCES == 0x40000`. This is the same failure *class* #2504 closed on the upload-failure axis — *"the indirect buffer's contents are fetched and executed by the GPU … that's a page-fault/TDR risk, not a misrender"* — left open on the overflow axis.
- **Evidence**: producer `let count = draws.len().min(MAX_INDIRECT_DRAWS);` with a one-shot warn; consumer `let mut i = 0; while i < batches.len() { … self.device.cmd_draw_indexed_indirect(cmd, indirect_buffer, byte_offset, group_size, indirect_stride); }`. `should_use_indirect_draws(global_bound, multi_draw_indirect_supported, indirect_upload_ok)` gates the path but has **no count limb**.
- **Impact**: If reached, the recorded command violates VUID-vkCmdDrawIndexedIndirect-offset-00556 (`offset + drawCount × stride` must be ≤ buffer size) and the GPU fetches `indexCount` / `vertexOffset` / `firstInstance` from unallocated memory — device-lost class, not a misrender. Reachability is a deep tail: it needs **> 262 144 post-merge rasterized batches in one frame**, roughly 20× the densest cell the codebase's own comments cite ("12k DrawCommands"), and `skip_batch` already keeps off-frustum / water draws out of `batches`. Classified MEDIUM (defence-in-depth gap at an already-declared overflow ceiling) rather than the HIGH floor a live spec violation would take, because the regime is one RP-1 has already declared lossy.
- **Related**: #2504, #647 / RP-1, #309, #1581 / F1.
- **Suggested Fix**: Clamp the loop bound — `let max_batches = batches.len().min(MAX_INDIRECT_DRAWS);` when `use_indirect` is true — or fold a `batches.len() <= MAX_INDIRECT_DRAWS` limb into `should_use_indirect_draws` so an overflowing frame falls back to direct draws (which read no indirect buffer at all). **Pure-Rust change with a unit-testable predicate; no barrier or render-pass edit, so it needs no capture.**

#### REN-D13-01: `taa.comp` masks bit 31 off before comparing mesh IDs, cross-comparing two different ID namespaces

- **Severity**: MEDIUM
- **Dimension**: 13 — TAA. See **Cluster A**.
- **Location**: `crates/renderer/shaders/taa.comp` — the `disocclusion` predicate and the 5-tap dilation loop's `candidateSurface` test
- **Status**: NEW
- **Description**: Two sites compare across the namespaces. (1) `bool disocclusion = currSurface != (prevMid & 0x7FFFFFFFu);` — an opaque pixel whose `entity_id + 1` numerically equals some previous-frame blended fragment's `draw_index + 1` is accepted as "same surface", and `sample_history_catmull_rom` pulls that transparent fragment's resolved colour into the opaque surface. The `alphaBlend` early-out inspects only the **current** fragment, never the previous one. (2) `uint candidateSurface = texelFetch(uCurrMeshId, p, 0).r & 0x7FFFFFFFu;` in the surface-constrained motion dilation — a colliding blended neighbour is admitted as "the same stable surface" and its motion vector can win the `lenSq > maxLenSq` test, which is precisely the foreground-motion-leaks-into-background case the dilation's own comment says the constraint exists to prevent.
- **Evidence**: The shader's `#904` comment still justifies the mask with *"a same-surface opacity transition (alpha-blended ↔ opaque)"*, which was true when both encodings were the instance index; `git show 883f57cd -- crates/renderer/shaders/taa.comp` is **comment-only**, so the predicates were never revisited. Because the blend pipeline also overwrites the **normal** attachment, the `surfaceMismatch` guard (`dot(currNormal, prevNormal) < 0.85`) reads the *billboard's* normal, not the occluded wall's — a camera-facing billboard and a camera-facing wall routinely pass a 0.85 cone, so the two guards fail together rather than covering for each other. The YCoCg variance clip bounds the *magnitude* of the ghost but does not reject it: a translucent double image inside `mean ± 1.5σ` survives.
- **Impact**: Translucent double images / trailing ghosts on opaque geometry that a particle or effect card crossed on the previous frame, plus occasional wrong motion vectors at particle silhouettes. Concentrated on specific (draw-index, entity-id) pairs, so it reads as a **fixed smear rather than general noise**. Visual only. **Candidate contributing mechanism for the unresolved "diagonal double-image / translucency ghost in Skyrim interiors" note** — offered as a mechanism, not a diagnosis, since no capture was taken. Reduced exposure: `UpscalerMode::default()` is `Fsr3(Quality)`, so `self.taa` is `Some` only under `--upscaler taa` or FSR-startup-failure promotion; this would be one severity step higher if TAA were the default again.
- **Related**: `883f57cd`, #904, #992, #2466, REN-D8-NEW-01 (identical predicate shape in `svgf_temporal.comp` / `svgf_atrous.comp` — **fix both together**).
- **Suggested Fix**: Treat `(prevID & 0x80000000u) != 0u` (and the dilation's candidate equivalent) as an outright non-match rather than masking the bit away. Cannot regress #904's motivating case, for the reason given in REN-D8-NEW-01.

#### REN-D13-02: geometry silhouettes against the sky can never accumulate TAA history, so Halton jitter converts static edge aliasing into per-frame edge crawl

- **Severity**: MEDIUM
- **Dimension**: 13 — TAA
- **Location**: `crates/renderer/shaders/taa.comp` — the `background` (`currSurface == 0u`) and `disocclusion` early-outs; `crates/renderer/shaders/composite.frag` — the `is_sky` branch; the mesh-ID clear value in `crates/renderer/src/vulkan/context/draw.rs`
- **Status**: NEW
- **Description**: Sky is **synthesised inside `composite.frag`** (`bool is_sky = !has_surface && (params.depth_params.x > 0.5);` → `combined = compute_sky(dir);`) and never exists in the HDR attachment TAA operates on; sky pixels in TAA's input hold only the render-pass clear colour, and their mesh ID holds the clear value `0`. At a geometry/sky silhouette, sub-pixel Halton jitter flips whether the pixel centre is covered, and **both** possible states reject history: covered → `currSurface = entity_id + 1` against a reprojected `prevMid` of `0` ⇒ `disocclusion`; uncovered → `currSurface == 0u` ⇒ `background`. With a parked camera the motion vector is exactly zero (both clip positions are un-jittered in `triangle.vert`), so `prevUv == uv` and the pixel **re-disoccludes itself every single frame** — a *permanent* disocclusion, not the one-frame kind the test is designed for. The pixel alternates between shaded geometry and `compute_sky` at the jitter period (16) with no temporal resolve at any point. Without TAA (`jitter == (0,0)`) the same pixel is stable.
- **Evidence**: `taa.comp`'s reject list `offscreen || background || disocclusion || surfaceMismatch || alphaBlend`; `composite.frag`'s `is_sky` branch discards the sampled `hdrTex` value entirely for those pixels; `draw.rs`'s `clear_values` clears mesh_id to `uint32: [0,0,0,0]` with the comment *"Mesh ID: 0 reserved for background"*; `triangle.vert` applies `currClip.xy += jitter.xy * currClip.w;` **after** `fragCurrClipPos = currClip;`. This is also the one class of edge where the neighbourhood clamp cannot substitute for accepted history, because the sky side of the edge is not present in TAA's image at all — its 3×3 moments see the clear colour, not `compute_sky`'s radiance.
- **Impact**: Visible pixel-level edge crawl along every exterior geometry-against-sky silhouette in `UpscalerMode::Taa`, **including with the camera stationary** — the highest-contrast, most visible aliasing case in an exterior scene, and the one TAA is nominally there to fix. Enabling TAA makes these specific edges *worse* than leaving jitter off. Interiors are unaffected (`is_sky` additionally requires the exterior flag). Visual only.
- **Confidence**: the mechanism is traced end-to-end in source and each link is grep-confirmed, but the *perceptual magnitude* has not been measured — **needs visual verification** (see §7) before sizing a fix.
- **Related**: #2466 (composite's `is_sky` branch discarding alpha-blended geometry drawn against the sky — same "sky is not in the G-buffer" root condition, different consumer); REN-D16-01 (bloom reads the same sky-free HDR attachment — **third consumer of the same root condition**).
- **Suggested Fix**: Direction only, not a prescription — instead of a hard history reject when `prevMid == 0` while `currSurface != 0` (and the mirror case), accept the history sample but force the tightest clamp (e.g. collapse `gamma` toward 0 so the sample is clipped to the current neighbourhood mean). That preserves anti-aliasing on jitter-driven coverage flips while still refusing to import off-surface colour. Any change must be re-checked against `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces`, which pins the exact reject-list expression.

#### REN-D14-NEW-01: `INSTANCE_FLAG_CAUSTIC_SOURCE` is set on draws whose pixels the splat shader unconditionally rejects (opaque / alpha-tested glass, opaque MultiLayerParallax)

- **Severity**: MEDIUM
- **Dimension**: 14 — Caustics. See **Cluster B, Mechanism 2**.
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`is_caustic_source`, `is_refractive_glass`, and the `f |= INSTANCE_FLAG_CAUSTIC_SOURCE` site) vs. `crates/renderer/shaders/caustic_splat.comp` (the bit-31 mesh-ID gate) and `crates/renderer/shaders/triangle.frag` (the `outMeshID` write)
- **Status**: NEW
- **Description**: The CPU gate and the GPU gate disagree about what a caustic source is. `is_caustic_source` consults **only** `material_kind` and `multi_layer_refraction_scale` — never `alpha_blend`. The shader rejects every pixel whose mesh-ID bit 31 is clear, and that bit is written from `INSTANCE_FLAG_ALPHA_BLEND` alone. So any caustic-source instance that is not alpha-blended carries the flag and can never splat. The shader's own comment asserting the opposite is wrong: *"caustic sources are always alpha-blend (the post-#922 CPU gate restricts CAUSTIC_SOURCE to MATERIAL_KIND_GLASS and MultiLayerParallax refraction, both of which require alpha-blend upstream)"*.
- **Evidence**: Neither accepted signal requires alpha-blend. `MATERIAL_KIND_GLASS` is assigned by `classify_glass_into_material` (`byroredux/src/helpers.rs`) gated on `has_transparent_coverage`, which `translate_material` (`byroredux/src/material_translate.rs`) feeds `source.has_alpha || source.alpha_test` — deliberately, per the classifier's own doc: *"Alpha-tested glass is deliberately allowed: broken-pane sheets use alpha test for shard coverage but still need dielectric shading."* MultiLayerParallax (kind 11) is accepted on `multi_layer_refraction_scale > 0.0` with no transparency condition at all. The **Cornell harness is 100 % affected**: `byroredux/src/cornell.rs`'s `glass()` probes state *"Glass is OPAQUE (no AlphaBlend)"*. The blind spot is untested — every fixture in `is_caustic_source_tests` is built by a `cmd()` helper hard-coded to `alpha_blend: true`.
- **Impact**: Silent, permanent loss of caustics for an entire content class — alpha-test broken panes, opaque MLP ice/glass, and every opaque-glass authoring the classifier is explicitly designed to admit — with no log, no telemetry and no test. It also makes the Cornell harness unusable as a caustic reference, which is the first tool an engineer would reach for. **The bit-31 gate itself is correct and must stay**: an opaque pixel's low bits are `inst.surfaceId`, not an instance index, so removing the gate would index `instances[]` out of range. The defect is that the CPU flag claims a capability the pipeline does not have.
- **Related**: #922 (the CPU gate tightening that introduced the asymmetry), #2515 (`glass()`'s unreachable `alpha:0.25` — same Cornell probe), REN-D11-2026-08-12-01 (**compounds** — fixing either alone leaves the pass dark), REN-D21-01.
- **Suggested Fix**: Decide which side owns the invariant and pin it. Cheapest honest fix: make `is_caustic_source` require `cmd.alpha_blend` so the flag stops lying, add the `alpha_blend: false` case to `is_caustic_source_tests`, and correct the `caustic_splat.comp` comment. If opaque glass *should* cast caustics, that is a larger change (the splat needs a per-pixel instance index for opaque pixels) and should be filed as its own enhancement rather than smuggled in here.

#### REN-D14-NEW-02: the `max_lights` budget bounds *accepted* lights, not ray queries — occluded lights trace for free and never charge the budget

- **Severity**: MEDIUM
- **Dimension**: 14 — Caustics / Performance
- **Location**: `crates/renderer/shaders/caustic_splat.comp` — the `for (uint li = 0u; li < lightCount && processedLights < maxLights; ++li)` loop and the `processedLights++` placement, against the documented bound on `is_caustic_source` in `crates/renderer/src/vulkan/context/draw.rs`
- **Status**: NEW
- **Description**: `processedLights++` executes **after** the visibility block, so a light that is inside the radius but shadowed hits `continue` inside `if (traceShadowBinary(...))` and never increments the counter — having already spent a `rayQueryEXT` traversal (two, when `visibilityMaskUsesGlass` is set, since the glass-layer trace runs as a second `traceShadowBinary`). The loop's real per-pixel bound is therefore `lightCount` traversals, not `maxLights` (uploaded as 8). `MAX_LIGHTS` is 512, so the worst case is ~2 orders of magnitude above the intended budget. Directional lights skip the radius test entirely and are culled only by the Lambert cosine.
- **Evidence**: the `needsVisibility` block ends `continue;` on an occluded trace with `processedLights++` sitting *below* it. `visibilityMaskNeedsTrace` (`crates/renderer/shaders/include/shadow_common.glsl`) is true for any light whose mask touches `VISIBILITY_MASK_ALL_OPAQUE | VISIBILITY_LAYER_GLASS`, i.e. every ordinary shadow-casting light. This contradicts the invariant the CPU gate's own doc leans on to justify how tight the classifier must be: *"the compute pass burns `max_lights` TLAS ray queries per flagged pixel, so the gate has to stay tight"* — the gate was tightened under #922 on the strength of a bound the shader does not enforce.
- **Impact**: Unbounded (up to `lightCount`) shadow-ray cost per caustic-source pixel in exactly the scenes where the cost is worst — dense interior lamp clusters where most lamps are occluded from a given bottle or pane. Blast radius is limited by how few pixels carry the flag, and today further masked by REN-D14-NEW-01 and REN-D11-2026-08-12-01 rejecting them before the loop is reached, which is why this is MEDIUM and not HIGH. **No frame-time claim is made — this was not measured.**
- **Related**: #922; REN-D14-NEW-01 and REN-D11-2026-08-12-01 (**fixing those increases the pixel count that reaches this loop** — sequence the fixes accordingly).
- **Suggested Fix**: Charge the budget for the traversal, not the acceptance — move `processedLights++` above the visibility block, or add a separate `tracedLights` counter compared against `maxLights` — and correct the `is_caustic_source` doc to state the bound the shader actually enforces.

#### REN-D15-01: water fragments that fail the depth test still shade and still deposit caustics — `water.frag` has no `early_fragment_tests`, and its `imageAtomicAdd` forbids the driver from adding one

- **Severity**: MEDIUM
- **Dimension**: 15 — Water
- **Location**: `crates/renderer/shaders/water.frag` (`main`, the `if (sunDirection.w > 0.0 && sceneFlags.x >= 0.5)` caustic block and the `traceWaterRay` calls above it); pipeline state in `crates/renderer/src/vulkan/water.rs` (`build_pipeline`, `depth_test_enable(true)` / `depth_write_enable(false)`)
- **Status**: NEW (sibling of the OPEN **#779**, which covers `triangle.frag` only, is perf-only, and does not cover the side-effect half of this)
- **Description**: `water.frag` writes a storage image (`imageAtomicAdd(waterCausticAccum, …)`). Per the Vulkan early-per-fragment-test rules an implementation may only hoist the depth test ahead of fragment shading when the shader has no such side effects, *or* when the shader declares the `EarlyFragmentTests` execution mode. `water.frag` declares neither. Consequently **every rasterized water fragment runs the full shader** — two `traceWaterRay` walks (each up to `MAX_TRANSPARENT_SKIPS = 8` ray-query iterations), `foamShoreline`'s ray, the sun `traceShadowTransmittance`, and the floor ray — and then atomically deposits into the caustic accumulator, *before* the depth test decides the fragment is invisible. The colour blend is correctly discarded by the late depth test; the caustic deposit is not, because it went to a storage image rather than an attachment.
- **Evidence**: `spirv-dis crates/renderer/shaders/water.frag.spv | grep OpExecutionMode` → `OpExecutionMode %main OriginUpperLeft` and nothing else; `grep -c early_fragment_tests crates/renderer/shaders/water.frag` → 0. The unguarded deposit sits at the tail of `main`. Contrast `caustic_splat.comp`, which is a compute pass driven by the G-buffer and therefore sources only from *visible* pixels by construction.
- **Impact**: Two effects, both exterior-only (the caustic block is gated on `sunDirection.w > 0.0`, so interiors are unaffected). **(1) Correctness / light leak**: a water plane occluded from the camera but lit by the sun — behind a wall, hill or building, the common exterior case since `spawn_water_plane` emits one 4096-unit quad per exterior cell — still projects its refracted floor hit to screen space. The `uv01` in-viewport guard checks that the hit *projects on screen*, **not** that the hit is the visible surface at that pixel, so the deposit brightens whatever occluder happens to be there: caustics land on the wall in front of the water. **(2) Cost**: the dominant per-fragment cost (the two `traceWaterRay` walks) is paid for the fully-occluded portion of every water quad in the frustum, which on an exterior grid is routinely the majority of a tile's rasterized area.
- **Related**: **#779** (OPEN, same execution-mode class, `triangle.frag` only); `caustic_splat.comp`'s G-buffer-sourced design as the contrasting sibling.
- **Suggested Fix**: Declare `layout(early_fragment_tests) in;` in `water.frag`. Water writes no depth and performs no `discard`, so the mode is semantically free on the blend side and is exactly what gates the storage-image write on visibility. **Needs RenderDoc / frame-timing verification before landing** (see §7): confirm the water blend is unchanged and measure the occluded-fragment saving on an exterior water cell.

#### REN-D15-02: water-side caustic deposits are single-pixel and cleared every frame, so they get neither of the two smoothing mechanisms `caustic_splat.comp` documents as load-bearing for the same composite term

- **Severity**: MEDIUM
- **Dimension**: 15 — Water
- **Location**: `crates/renderer/shaders/water.frag` (`main`, the `imageAtomicAdd` tail); `crates/renderer/src/vulkan/water_caustic.rs` (`WaterCausticAccum::clear_pre_render_pass`); consumed by `crates/renderer/shaders/composite.frag` (the `causticRadiance` block)
- **Status**: NEW
- **Description**: The glass-side writer spreads each deposit over a **5×5 normalised Gaussian footprint** and, when the camera is parked, runs a decay/EMA pass instead of a clear. Its own comment states why: *"thousands of those single-pixel deposits scatter into a sparse, grainy pool… The caustic is composited **after TAA**, so this footprint is its only smoothing."* The water-side writer does neither: `water.frag` deposits into exactly one `pixel`, and `clear_pre_render_pass` zeroes the whole per-FIF accumulator unconditionally every frame with no water analogue of the parked-camera decay branch. `composite.frag` then `texelFetch`es both accumulators and sums them into the *same* `causticRadiance` term with no filtering of its own.
- **Evidence**: `caustic_splat.comp` runs a `for (int ky = -2; ky <= 2; ++ky)` × `kx` loop weighting by `kGauss5[(ky+2)*5 + (kx+2)]` with a `size` bounds guard; `water.frag` does a bare `imageAtomicAdd(waterCausticAccum, pixel, fixed_val);`. `clear_pre_render_pass` is the only per-frame state op on the water accumulator — `vkCmdClearColorImage` to all-zero, every frame, no decay branch. `composite.frag` sums them: `vec3 causticRadiance = (glassCausticRaw + vec3(float(waterCausticRaw))) / CAUSTIC_FIXED_SCALE;`
- **Impact**: The water half of the shared caustic term is expected to read as salt-and-pepper speckle rather than a focused pool, and to shimmer frame-to-frame, because the landing point is scattered by `Nperturbed` — high-frequency normal-map / value-noise detail — and nothing downstream averages it (composite runs after TAA). Exterior sunlit water only. Visual-quality only; no crash, no corruption. The magnitude is a shader-cost decision, which is why this is reported as a structural asymmetry rather than with a prescribed kernel.
- **Related**: **#2468** (OPEN — parked-camera caustic EMA has no dynamic-scene invalidation; that concerns the *glass* decay branch, which water has no counterpart to at all); #1210 Phase D / #1256 (shipped the single-pixel deposit; the glass footprint landed later and was never mirrored).
- **Suggested Fix**: Give the water deposit the same normalised footprint the glass path uses — the `kGauss5` table and the `size` bounds guard are already written and can be shared through `crates/renderer/shaders/include/` rather than duplicated. Decide separately whether the parked-camera decay branch should also apply to the water accumulator, or document why per-frame clear is correct for it. **Confirming signal** in §7.

#### REN-D16-01: bloom's source attachment contains no sky, so the pyramid is seeded with the exterior clear colour and real sky/sun radiance never blooms

- **Severity**: MEDIUM
- **Dimension**: 16 — Bloom
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs` (`record_bloom_pass` → `bloom.dispatch(&self.device, cmd, frame, hdr_view)` with `hdr_view = composite.hdr_image_views[frame]`); `crates/renderer/shaders/composite.frag` (`is_sky` branch, and the later unconditional `combined += bloom * BLOOM_INTENSITY`); `byroredux/src/main.rs` (`clear_color`); `crates/core/src/types.rs` (`Color::CORNFLOWER_BLUE`)
- **Status**: NEW — follow-up to CLOSED #2233, **not** a regression of it (#2233's fix, the unconditional add, is intact; the *source* is the gap)
- **Description**: The sky is synthesised inside `composite.frag` and never exists in the HDR G-buffer attachment that bloom reads. Two consequences. (1) **The sun disc and bright sky can never bloom** — #2233 made the add unconditional precisely so sky pixels would receive bloom, and `composite.frag` still carries that rationale verbatim (*"previously geometry-branch-only, so sky pixels — e.g. a bright sun disc — never bloomed"*), but the pyramid holds no sky radiance to deliver. (2) **A debug clear colour is injected into the scene** — for exteriors `clear_color = Color::CORNFLOWER_BLUE = (0.392, 0.584, 0.929)` (interiors clear to black, so this is exterior-only), so every sky pixel of the bloom *source* is that constant.
- **Evidence**: `record_bloom_pass`'s own doc block states the input is *"the raw pre-TAA HDR attachment (`composite.hdr_image_views[frame]`)"*, and `rebind_hdr_views` only rewrites composite's descriptor, never the `hdr_image_views` field, so bloom's input is always the raw attachment. `composite.frag`'s `is_sky` branch never reads `hdrTex`. Per `bloom_upsample.comp`'s own documented DC gain (`up[0] = 5V` for a locally-constant region), composite adds ≈ `5 × 0.15 × (0.392, 0.584, 0.929) = (0.29, 0.44, 0.70)` linear HDR to sky pixels and — through the blur footprint — bleeds it across horizon silhouettes. This lands *upstream* of `presentation.frag`'s ACES, so it is a genuine exposure lift, not a clamped artefact. Indirect (SVGF) and both caustic accumulators are likewise absent from the bloom source for the same structural reason: bloom sees **direct only**.
- **Impact**: Exterior-only. Sky washed toward blue and lifted ~0.3–0.7 linear; glow bleeds around horizon geometry; and the effect #2233 was filed to enable is absent exactly where it matters most. The structural half is established from code alone; the magnitude figure is analytic and wants a capture (§7).
- **Related**: #2233 (CLOSED), #2466 (OPEN — the same `is_sky` branch also discards alpha-blended geometry drawn against the sky), REN-D13-02 (**third consumer of the same root condition**), #1166, #1107, REN-D16-06 (the same 5× gain is what turns the clear-colour plateau into a visible lift).
- **Suggested Fix**: Either clear the exterior HDR attachment to black and accept that sky does not bloom (one line, zero risk, kills the colour injection), or move bloom downstream of composite into its own HDR pass so it sees the assembled scene including sky, GI and caustics. The second is what the #2233 rationale actually requires.

#### REN-D16-03: `memory-budget.md` states the bloom pyramid is not FIF-doubled; `BloomPipeline` allocates a complete pyramid per frame-in-flight

- **Severity**: MEDIUM
- **Dimension**: 16 — Bloom / Memory-Lifecycle
- **Location**: `docs/engine/memory-budget.md` ("### Bloom" section + the "Bloom pyramid" row of the VRAM Rough Budget table); `crates/renderer/src/vulkan/bloom.rs` (`BloomFrame`, `BloomPipeline::frames`, `BloomFrame::new`)
- **Status**: NEW (the row was introduced by #1872's fix, which is where the wrong claim entered)
- **Description**: The doc says the pyramid is *"recomputed every frame with no history — **not FIF-doubled, unlike everything else on this page**"* and budgets ~3.5 MB at 1080p / ~13.8 MB at 4K. The code allocates `MAX_FRAMES_IN_FLIGHT` independent `BloomFrame`s, each owning its own `BLOOM_MIP_COUNT` down images *and* `BLOOM_MIP_COUNT - 1` up images. Being per-FIF is in fact **required**: `dispatch()` rewrites `down_descriptor_sets[0]` binding 0 every frame and writes every mip with no pre-barrier, which is only sound because each slot's images are exclusive to that slot and gated by the frame fence (#931's rationale).
- **Evidence**: `bloom.rs` — `for frame_idx in 0..MAX_FRAMES_IN_FLIGHT { … partial.frames.push(frame); }`, with descriptor-pool sizing `MAX_FRAMES_IN_FLIGHT * BLOOM_MIP_COUNT` / `MAX_FRAMES_IN_FLIGHT * (BLOOM_MIP_COUNT - 1)` and a fresh `create_mip(...)` per level per frame. Byte math at a 1920×1080 **render** extent, `B10G11R11_UFLOAT_PACK32` = 4 B/px: down = 960×540 + 480×270 + 240×135 + 120×67 + 60×33 = 690 420 px; up = the first four of those = 688 440 px; total 1 378 860 px × 4 B ≈ **5.52 MB per FIF → ~11.0 MB** (doc: ~3.5 MB). At 3840×2160 render extent: 5 516 040 px × 4 B ≈ 22.1 MB per FIF → **~44.1 MB** (doc: ~13.8 MB). Half-resolution base confirmed (`(screen_extent.width / 2).max(1)`), and `screen_extent` is `frame_extents.render` at both call sites, so the figures scale with render extent and shrink under any FSR preset.
- **Impact**: ~3.2× understatement on both budget rows. Small in absolute terms (≈ +7.5 MB at 1080p, +30 MB at 4K) but it is a **wrong invariant** in the document audits are instructed to treat as authoritative — and "not FIF-doubled" is exactly the kind of sentence a future refactor would cite to justify collapsing `frames` to one pyramid, reintroducing the cross-frame WAR that #931's barrier reduction depends on being absent.
- **Related**: #1872 (added the row), #931 (the per-FIF-exclusivity argument), REN-D5-02 / REN-D16-02 (sibling drift in the same doc), #2679.
- **Suggested Fix**: Correct the section to "one pyramid per frame-in-flight (required for #931's barrier reduction)", double the two figures, and update the VRAM Rough Budget row.

#### REN-D16-04: the froxel temporal "neighbourhood clamp" is computed from the history volume itself, and the emissive fast time-constant keys only on the current sample

- **Severity**: MEDIUM
- **Dimension**: 16 — Volumetrics
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp` — the `params.prev_camera_pos.w > 0.5` / `reprojectHistory` block (`historyMean`, `historySecondMoment`, `historySigma`, `emissionFraction`, `emissionAgreement`, `historyWeight`); constants `DEFAULT_TEMPORAL_HISTORY_WEIGHT`, `DEFAULT_EMISSIVE_HISTORY_WEIGHT`, `DEFAULT_DENSITY_REJECTION` in `crates/renderer/src/vulkan/volumetrics.rs`
- **Status**: NEW (mechanism landed in `edbed7a3`, never audited)
- **Description**: Two coupled observations. (1) The 3×3 statistics that clamp `history.rgb` are gathered by sampling `previousFroxel` — the **history volume itself** — not the current frame. Unlike `taa.comp` (moments from `uCurrHdr`) and `svgf_temporal.comp` (firefly statistic from `currIndirectTex`), clamping a value to a neighbourhood that *includes that value* removes single-froxel spatial spikes but places **no bound whatever on disagreement with the current frame**; a spatially smooth but temporally stale history passes through untouched. (2) The only genuine current-vs-history rejections are therefore the density term (`exp(-temporal_params.y * relativeDensityDelta)`, `DEFAULT_DENSITY_REJECTION = 4.0`) and `emissionAgreement = exp(-2.5 * relativeRadianceDelta * emissionFraction)`. The second is multiplied by `emissionFraction`, derived from the **current** sample alone. On the *trailing* edge — a froxel a flame has just left — `emissionFraction → 0`, so `emissionAgreement → 1` **and** the `mix(steadyWeight, emissiveWeight, emissionFraction)` selects `steadyWeight = 0.92` (~12-frame decay) rather than `emissiveWeight = 0.75` (~3.5 frames). The emissive time constant is asymmetric: **fast on, slow off**.
- **Evidence**: The clamp statistics loop samples `previousFroxel`; `emissionFraction` is `emissionLuma / sourceLuma` of this frame's `local_medium.emission`. **Partially self-disproved, stated for honesty**: when the departing volume also dominated that froxel's extinction, `relativeDensityDelta ≈ 1` and `exp(-4) ≈ 0.018` collapses `historyWeight`, suppressing the trail. The case that survives is a froxel where the ambient/global medium dominates `sigma_t` — fogged exteriors, or a bright but optically thin flame — where the density delta is small and only the now-zero emission-gated term could have rejected the stale radiance. **Non-findings established while checking this (do not re-derive)**: `historyWeight` is a `clamp(_, 0, 0.98)` times two `exp(-x) ≤ 1` factors and `mix(current, history, w<1)` is a contraction with fixed point `current`, so no runaway accumulation; magnitudes are far below the RGBA16F 65504 ceiling; the previous-slot index is the other FIF slot, barriered, so no slot reads its own in-flight write; `history_valid` starts false and gates the whole block via `prev_camera_pos.w`.
- **Impact**: Emissive smear trailing moving fire/explosion fronts in fogged exteriors. Visual only, bounded.
- **Related**: `edbed7a3`, #2470 (a *different* Z-convention issue on the integrated volume — `reprojectHistory`'s slice convention is correct for the injection volume), #2241 (OPEN, sibling integration-side over-brightening).
- **Suggested Fix**: Gather the clamp statistics from the current frame's computed `inscatter`/`extinction` neighbourhood (requires a shared-memory or second-pass restructure), or — much cheaper — derive `emissionFraction` from `max(current emission luma, reprojected-history emission content)` so the trailing edge inherits the same short time constant as the leading edge.

#### REN-D17-05: Disney sheen tint multiplies raw albedo instead of the luminance-normalised tint, diverging from the cited GLSL-PathTracer / Disney-2012 reference

- **Severity**: MEDIUM
- **Dimension**: 17 — Disney BSDF
- **Location**: `crates/renderer/shaders/include/pbr.glsl` — `disneyDiffuseSplit` (the `sheenColor` line). Mirror docs: `GpuMaterial::sheen_tint` (`crates/renderer/src/vulkan/material.rs`) and `Material::sheen_tint` (`crates/core/src/ecs/components/material.rs`).
- **Status**: NEW
- **Description**: `disneyDiffuseSplit` builds its sheen colour as `mix(vec3(1.0), albedo, sheenTint)`. Both cited references build it from a **luminance-normalised** tint. Disney 2012 (`disney.brdf`) computes `Cdlum = .3r + .6g + .1b`, `Ctint = Cdlum > 0 ? baseColor/Cdlum : vec3(1)`, `Csheen = mix(vec3(1), Ctint, sheenTint)`; knightcrawler25/GLSL-PathTracer — the reference this function's own doc block names *verbatim* ("`EvalDisneyDiffuse`") — does the same in `GetSpecColor`. The normalisation exists precisely so `sheenTint` transfers **hue** without changing sheen *intensity*. Using raw albedo couples the two: at `sheenTint = 1.0` a dark base colour (albedo ≈ 0.05, e.g. black velvet — the canonical sheen material) scales the sheen lobe down by ~20×, and a base colour above 1.0 scales it up.
- **Evidence**: `vec3 sheenColor = mix(vec3(1.0), albedo, sheenTint);` … `o.sheen = FH * sheen * sheenColor;`, consumed unchanged at both gate sites (`crates/renderer/shaders/include/lighting.glsl`'s `shadowableLightRadiance` and `triangle.frag`'s `lightCount == 0` fallback). **Every other term of this function was verified to reproduce the reference exactly** — `Fd + Fretro` algebraically equals `mix(1,Fd90,FL)·mix(1,Fd90,FV)` with `Fd90 = 0.5 + 2·roughness·HdotL²`, and `Fss90 = 0.5·Rr = roughness·HdotL²` matches GLSL-PathTracer's `Fss90 = HdotL²·roughness` — which is what makes this one line stand out rather than read as a deliberate simplification. No comment marks it as one.
- **Impact**: Wrong sheen magnitude on any tinted-sheen material, in a lobe whose whole purpose is cloth/silk/velvet. Blast radius today is bounded: `sheen`/`sheen_tint` have **no source-format producer** — the single NIFAL boundary writes `sheen: 0.0, sheen_tint: 0.0` literally (#2514) — so the only reachable producer is the `mat.set sheen_tint …` console arm driving the Cornell harness. That makes this a *latent* correctness defect **on the reference-validation path**: the harness built to validate the Disney lobe would validate a lobe that does not match its own reference. It activates the moment a sheen producer (BGSM v9+ / Starfield `.mat`) lands.
- **Related**: #2514 (the `mat.set`-only reachability of these four scalars); #2489 (`mat.set` writes canonical PBR scalars with no clamp or finite guard — `sheenTint > 1` also extrapolates through this `mix`); the earlier π-scaling defect in the same lobe (`docs/audits/AUDIT_RENDERER_2026-05-24_DIM6_14.md`).
- **Suggested Fix**: Compute the tint the way both references do — `float lum = dot(albedo, vec3(0.3, 0.6, 0.1)); vec3 ctint = lum > 0.0 ? albedo / lum : vec3(1.0);` then `sheenColor = mix(vec3(1.0), ctint, sheenTint)` — or, if the raw-albedo form is intentional, say so in the doc block and stop citing `EvalDisneyDiffuse` for it.

#### REN-D18-01: a mid-session worldspace transition renders one frame of TOD_DAY sky + full-intensity sun at any game hour

- **Severity**: MEDIUM
- **Dimension**: 18 — Sky/Weather
- **Location**: `byroredux/src/scene/world_setup.rs` — `apply_worldspace_weather` (the `insert_resource` pair in the WTHR arm); producers `byroredux/src/env_translate.rs` (`translate_exterior_cell_lighting`, `translate_sky`, `const SUN_INTENSITY`). Call sites: `byroredux/src/app_step.rs` (`step_cell_transition`, the `TransitionDestination::Exterior` arm) and `byroredux/src/debug_load.rs`.
- **Status**: NEW
- **Description**: `7a851ab9` made the bootstrap **sun direction** honour the live clock (`bootstrap_game_hour` → `compute_sun_arc`), but the palette half of the same seed was left at the fixed `TOD_DAY` slot and `sun_intensity` at the constant `SUN_INTENSITY = 4.0`. So the seed is internally inconsistent: sun **vector** = live hour, everything else = noon. At boot this is invisible — `byroredux/src/main.rs` runs the scheduler immediately after `setup_scene()`, so `weather_system` corrects the seed before the first rendered frame. On a **mid-session worldspace change** it is not: `step_debug_loads` / `step_cell_transition` run *after* `self.scheduler.run(&self.world, dt)` and *before* `self.render_one_frame(...)` in the same `about_to_wait` iteration, so the seeded values are what `build_render_data` uploads for that frame.
- **Evidence**: the WTHR arm derives `sun_dir` from `bootstrap_game_hour(world)` but calls `translate_exterior_cell_lighting(wthr, sun_dir)` and `translate_sky(wthr, sun_dir, …)`, which read `sky_colors[…][TOD_DAY]` and `fog_day_near/far` unconditionally. Consumer chain for that frame: `byroredux/src/render/sky.rs::build_sky_params` copies `sun_intensity` straight through; `byroredux/src/render/lights.rs::collect_lights` snapshots it and feeds `compute_directional_upload`, whose exterior arm scales by `(sun_intensity / SUN_INTENSITY_PEAK).clamp(0,1)` — `4.0 / 4.0 = 1.0`.
- **Impact**: Door-walk into an exterior worldspace at, say, 01:00 and one frame is composited with the daytime zenith/horizon/lower gradient, daytime fog colour and distance, the TOD_DAY `SKY_SUNLIGHT` directional at **full** strength, and a `directional_dir` of `[0, -1, 0]` (the below-horizon sentinel `compute_sun_arc` correctly returns at night) — a full-brightness key light pointing straight down under a noon sky. This is the pre-#798 failure mode, fixed for the steady state, resurfacing for exactly one frame. Exterior-only; visual, no corruption. Whether it also poisons TAA/SVGF history beyond that frame is a capture question (§7), not a `cargo test` one.
- **Related**: `7a851ab9` (the sun-direction half of this seed); #2511 (the adjacent transition-lifecycle fix, verified intact); `docs/engine/exal.md` §2, which calls the old seed "dead for one frame and misleading" — it is not dead.
- **Suggested Fix**: After `apply_worldspace_weather` returns on the transition path, resample once with the idiom that already exists for the console clock — `crate::systems::weather_system(world, 0.0)` (`byroredux/src/commands/time.rs::resample_lighting`). A `dt = 0.0` tick advances no clock and no cross-fade, so it renders the correct TOD sample of whichever weather the fade should start from. Alternatively hand the TOD slot pair into the two translate functions instead of hardcoding `TOD_DAY`. **CPU-side; no render-pass change is implied.**

#### REN-D18-02: the Skyrim DALC ambient cube is excluded from the WTHR cross-fade blend

- **Severity**: MEDIUM
- **Dimension**: 18 — Sky/Weather
- **Location**: `byroredux/src/systems/weather.rs` — `weather_system`, the `let dalc_cube = …` binding between `drop(wd)` and the `SkyParamsRes` write; the blend it is missing from is the `if transition_t > 0.0 { … }` tuple above
- **Status**: NEW (raised in the quarantined 11:44 pass; never filed, **re-verified still present at `e4ab12e8`**)
- **Description**: `weather_system` blends ten quantities across an in-flight `WeatherTransitionRes` cross-fade (zenith, horizon, lower, sun_col, ambient, sunlight, fog_col, fog_near, fog_far, fog_medium). The eleventh per-weather field written into `SkyParamsRes` — `current_dalc_cube` — is computed **outside** that blend: it re-reads the live `WeatherDataRes` (the *source* weather only) and never samples `tr.target.skyrim_dalc_per_tod`. The target's cube therefore arrives as a single-frame snap when `promote_weather_transition_target` runs at `t >= 1.0`, instead of easing in over the 8-second fade.
- **Evidence**: the binding reads `world.try_resource::<WeatherDataRes>()` → `.skyrim_dalc_per_tod` → `DalcCubeYup::lerp(&cubes[fold(slot_a)], &cubes[fold(slot_b)], t)`; no `tr.target` read occurs anywhere in it. The field has a live consumer — `byroredux/src/render/sky.rs::build_sky_params` does `interior_cube.or_else(|| sky_res.current_dalc_cube.map(renderer_dalc_cube))`. Contrast `promote_weather_transition_target`, which *does* promote `skyrim_dalc_per_tod` (#1102) — so the field is understood to be per-weather, just not blended.
- **Impact**: Skyrim-only (`skyrim_dalc_per_tod` is `None` on FNV/FO3/Oblivion). Across any worldspace change between two Skyrim weathers with differing DALC cubes, the six-axis ambient cube holds the old weather's value for the whole fade and pops on the completion frame while every other sky/lighting quantity eases. Visual only.
- **Related**: same asymmetry class as #1018 (target night-factor for fog distance) and #1101 / #1102 (wind + DALC promotion).
- **Suggested Fix**: Inside the existing `if transition_t > 0.0` block, sample `tr.target.skyrim_dalc_per_tod` at the target's own `(b_a, b_b, b_t)` slots and `DalcCubeYup::lerp` against the source cube by `transition_t`, mirroring the `target_fog_*` treatment directly above. Decide the `Some`/`None` mismatch explicitly — source-with-DALC → target-without should fade out, not snap.

#### REN-D18-03: `build_tod_keys`'s night anchor is clamped by an unsourced `23.0` that fires on vanilla FNV/FO3 and can go non-monotonic

- **Severity**: MEDIUM
- **Dimension**: 18 — Sky/Weather
- **Location**: `byroredux/src/systems/weather.rs` — `build_tod_keys`, the `let night = (sunset_end + 2.0).min(23.0);` binding (key 6). Guard corpus: `tod_keys_are_monotonic_on_realistic_climates`.
- **Status**: NEW (sibling of OPEN **#2473**, which covers key 4's `afternoon_cool` clamp only — different key, different trigger)
- **Description**: Two problems with one literal. **(a)** The documented model in the function's own doc comment is *"`sunset_end + 2h` (clamped to 23h) → `TOD_NIGHT`"*. Every shipped Fallout climate has `sunset_end = 22.0` (FNV `[6, 10, 18, 22]`, FO3 Capital Wasteland `[5.333, 10, 17, 22]`), so `22 + 2 = 24` is clamped to `23` **on vanilla content**: the interpolator reaches full `TOD_NIGHT` an hour earlier than the `+2h` rule states, compressing the `SUNSET → NIGHT` ease from 6 h to 5 h. The clamp's stated purpose (staying below the `keys[0] + 24 = 25.0` wrap point) is satisfied by anything under 25.0, so `23.0` is 2 hours stricter than the constraint requires with no source cited — `feedback_no_guessing.md` territory for a value that changes rendered colour timing on shipped data. **(b)** Because the clamp is absolute rather than relative to its predecessor key 5 (`sunset_begin`), any climate with `sunset_begin > 23.0` produces `keys[5] > keys[6]`. `climate_tod_hours` validates TNAM bytes only against `1..=144`, so bytes 139–144 (`23.17h`–`24.0h`) pass validation and yield a non-monotonic table — exactly the invariant `pick_tod_pair` assumes and #2473 documents the consequences of.
- **Evidence**: `let afternoon_cool = (sunset_begin - 2.0).max(sunrise_end + 0.1); // key 4 — #2473` sits directly above `let night = (sunset_end + 2.0).min(23.0); // key 6`. `tod_keys_are_monotonic_on_realistic_climates`'s four-entry corpus tops out at `sunset_begin = 19.5`, so it cannot catch (b); no test asserts (a) at all.
- **Impact**: (a) affects every FNV/FO3 exterior every in-game evening — a subtle but real deviation from the documented TOD model, in the same palette/fog/sun lockstep machinery #897/#1012 exist to keep coherent. (b) is modded/authored-CLMT only, visual, self-correcting on the next segment.
- **Related**: #2473 (key 4, same table, same invariant), #463 / #530 (`climate_tod_hours` validation range), #897.
- **Suggested Fix**: Fold into #2473's fix — clamp each key against its true predecessor (`night = (sunset_end + 2.0).max(sunset_begin + 0.1).min(24.9)`) and extend `tod_keys_are_monotonic_on_realistic_climates` to a full `windows(2)` assertion over a corpus that includes a late-sunset climate (`[6.0, 10.0, 23.5, 24.0]`). If the 1-hour vanilla compression in (a) is intentional, record why in the doc comment instead of leaving the literal unexplained.

#### REN-D19-01: LAND terrain ships `bitangent_sign = +1` but its UV parametrization requires `−1` — every TX01 splat normal map is V-inverted

- **Severity**: MEDIUM
- **Dimension**: 19 — Tangent-Space
- **Location**: `crates/renderer/src/vertex.rs` — `Vertex::new_terrain` (`tangent: [1.0, 0.0, 0.0, 1.0]`); UV/position source `byroredux/src/cell_loader/terrain.rs` (the `for row` / `for col` vertex loop); consumer `crates/renderer/shaders/include/material_sampling.glsl` — `perturbNormal` Path 1
- **Status**: NEW
- **Description**: `new_terrain` hard-codes `w = +1.0`. The engine's handedness convention is fixed by `bitangent_sign` (`crates/nif/src/types.rs`), derived as `sign(dot(∂P/∂V, cross(N, ∂P/∂U)))` — i.e. the shader's `B = w · cross(N, T)` must reproduce **+∂P/∂v**. For the synthesized LAND grid it reproduces **−∂P/∂v**, so the reconstructed bitangent points the wrong way and the normal map's green (V) axis is inverted on all near-field terrain.
- **Evidence**: In `terrain.rs` the vertex loop builds `position = zup_to_yup_pos([origin_x + col·SPACING, origin_y + row·SPACING, height])`, and `zup_to_yup_pos` is `(x, y, z) → (x, z, −y)`, so `∂P/∂col ≈ +X` and `∂P/∂row ≈ −Z`. The same loop sets `uv = [col/32 · TILES, (1 − row/32) · TILES]`, so `∂u/∂col > 0` and `∂v/∂row < 0`. Chaining: `∂P/∂u = +X` ✓ (matches the stored `T = [1,0,0]`) and `∂P/∂v = (−Z)·(negative) = +Z`. `perturbNormal` computes `B = tangentSign · cross(N, T)`; with `N ≈ (0,1,0)`, `T = (1,0,0)`, `cross(N, T) = (0,0,−1)`, so `tangentSign = +1` yields `B = −Z` — the negation of the true `+Z`. The existing guard `terrain_vertex_carries_a_nonzero_tangent` asserts the exact tuple but only *justifies* the non-zero `xyz` (clearing `perturbNormal`'s `dot(T,T) > 1e-4` Path-1 gate, #2474); the `w` component's value was never derived.
- **Impact**: Every LAND cell that resolves a TX01 `_n` normal map (the `terrainSplatActive` loop in `triangle.frag` that calls `perturbNormal` per splat layer) shades with a mirrored V axis: north/south-facing micro-relief reads inverted (bumps as dents) while east/west relief is correct. Affects FNV / FO3 / Oblivion / Skyrim exterior ground wherever a splat layer has a normal map. Visual only. **Secondary site, same root cause, much lower impact**: `byroredux/src/cell_loader/water.rs` uses the same tuple on the water quad whose `uv` mirrors local `(x, z)`, so `water.frag`'s `B = normalize(cross(Nsurface, T) * vWorldBitangentSign)` is likewise negated — but water's normal field is procedurally scrolled and near-symmetric, so the artifact is a mirrored ripple pattern rather than wrong-looking relief. Worth fixing in the same change for convention consistency, not on its own merits.
- **Related**: #2474 (the closed zero-tangent terrain fix — guard intact); `bitangent_sign` / #1516.
- **Suggested Fix**: Change `Vertex::new_terrain`'s `tangent` to `[1.0, 0.0, 0.0, -1.0]` (and the water quad likewise), and extend `terrain_vertex_carries_a_nonzero_tangent` into a **derivation** guard asserting `w · cross(N, T) ≈ ∂P/∂v` for the LAND grid's actual `(row, col) → (position, uv)` mapping — the same style as the existing Rust-side shader-math guard `path2_cross_product_reconstructs_true_bitangent_under_uv_mirroring`. Alternatively drop the `(1.0 - row/32)` V flip in `terrain.rs`; **either change alone fixes it, both together re-break it.**

#### REN-D19-02: the `MAT_FLAG_MODEL_SPACE_NORMALS` branch overwrites the authored blue channel of three-channel FO4 `_msn` maps with `+sqrt(1 − x² − y²)`

- **Severity**: MEDIUM (escalates to HIGH the moment terrain `_msn` binding lands)
- **Dimension**: 19 — Tangent-Space
- **Location**: `crates/renderer/shaders/triangle.frag` — the `if ((mat.materialFlags & MAT_FLAG_MODEL_SPACE_NORMALS) != 0u)` arm inside the normal-map block
- **Status**: NEW
- **Description**: The branch applies the BC5 two-channel Z-reconstruction (`mn.z = sqrt(max(0.0, 1.0 - dot(mn.xy, mn.xy)))`) to model-space normal maps. That reconstruction is only valid for a tangent-space map, where `z > 0` holds by construction. A model-space normal legitimately has `z < 0` over roughly half of a closed mesh, and FO4's `_msn` textures are **not** BC5 — they are BC3/BC1 with a populated third channel. The branch therefore discards authored data and forces a non-negative Z, mirroring those normals through the model XY plane.
- **Evidence**: Decoded directly from the shipped FO4 archives (BA2 DX10 records; per-texture `dxgi_format` at base record byte 21, the same field `crates/bsa/src/ba2.rs` reads):

  | `_msn` population | count | DXGI | measured |
  |---|---|---|---|
  | `Textures\Terrain\…` | 6120 | 77 = `BC3_UNORM` | `\|2·RGB−1\|` median **0.994** (unit vector ⇒ all three channels authored); blue median 0.032, **45 % of texels have z < 0** |
  | `Textures\Actors\Character\FaceCustomization\…` | 1100 | 71 = `BC1_UNORM` | B = **0.000 ± 0.000** — genuinely two-channel, reconstruction *required* |
  | `…\Piper\PiperHead_msn.DDS` | 1 | 77 = `BC3_UNORM` | B 0.586 ± 0.276, **42 % of texels z < 0** |

  For comparison, plain tangent-space `_n` maps in the same archives are 878 × DXGI 83 = `BC5_UNORM`, i.e. the format the code comment assumes. On a typical terrain texel (`x ≈ 0.1`, `y ≈ 0.8`) the branch computes `z = +0.59` where the authored value is `≈ 0.03` — a ~36° error in the shading basis, not a rounding difference. Flag reachability is fully plumbed: `crates/bgsm/src/bgsm.rs` → `byroredux/src/asset_provider/material.rs` and the `TXST_FLAG_MODEL_SPACE_NORMALS` path in `byroredux/src/cell_loader/refr.rs` → `pack_imported_material_flags` → `MAT_FLAG_MODEL_SPACE_NORMALS`. **The measurements also settle a related question in the negative**: terrain `_msn` has mean G = 0.900 (green is the "up" axis), i.e. the maps are authored **Y-up**, matching the imported mesh space — so `mat3(inst.model) * mn` needs **no** additional Z-up→Y-up swap. Do not add one.
- **Impact**: Wrong shading normal on every three-channel `_msn` surface. Today the reachable vanilla-FO4 population is small (`PiperHead_msn` plus any modded or DLC three-channel `_msn`), because terrain `_msn` is not bound yet — `btr_normal_path` (`byroredux/src/cell_loader/terrain_lod_btr.rs`) resolves only `…_n.dds` and its module docs state the FO4 `_msn` variant "is not bound yet". **This becomes HIGH the moment that binding lands**, since it would put 6120 wrong-basis textures across the whole Commonwealth exterior. Visual only.
- **Related**: REN-D19-01 (the other wrong-basis site); `btr_normal_path`'s deferred FO4 `_msn` work.
- **Suggested Fix**: Make the reconstruction conditional rather than unconditional — keep the authored `mn.z` when the sampled blue carries signal and fall back to `sqrt(max(0, 1 − dot(mn.xy, mn.xy)))` only when it does not (the FaceCustomization BC1 case decodes to a constant `z = −1`, trivially separable from a real signed Z). Since the two encodings are distinguishable at *load* time from the DDS format, the cleaner boundary is a material flag set in the texture registry rather than a per-fragment heuristic. **Verify any shader-side change against a real `PiperHead_msn` capture** — not observable from `cargo test`.

#### REN-D19-03: `synthesize_tangents`'s Z-up degenerate branch never got the #2632 orthogonalize-and-normalize fix its Y-up sibling did

- **Severity**: MEDIUM
- **Dimension**: 19 — Tangent-Space
- **Location**: `crates/nif/src/import/mesh/tangent.rs` — `synthesize_tangents`, the `if vec3_is_zero(&tangent_zup) || vec3_is_zero(&bitangent_zup)` arm. Fixed sibling with the identical predicate: `synthesize_tangents_yup`.
- **Status**: NEW
- **Description**: Both synthesis functions fall back to nifly's "permute the normal's components" trick when a vertex accumulates a zero `∂P/∂u` or `∂P/∂v`. A raw cyclic permutation of `N` is **not** generally orthogonal to `N` (and is exactly `N` when its components are equal), so #2632 added a Gram-Schmidt projection + normalize before the cross product — **but only in `synthesize_tangents_yup`**. The Z-up flavour still emits the raw permutation.
- **Evidence**: `synthesize_tangents` (Z-up): `let t_z = [n_zup.y, n_zup.z, n_zup.x]; let t_y = zup_to_yup_pos(t_z); let b_y = cross(n_yup, t_y);` — no `dot(n, t)` projection, no `normalize_inplace`. `synthesize_tangents_yup` (fixed): builds `t_y_raw`, subtracts `n_yup * dot_nt`, calls `normalize_inplace(&mut t_y)`, *then* crosses. The asymmetry is mirrored in the test suite: `synthesize_tangents_yup_degenerate_fallback_normalizes_and_orthogonalizes_against_n` exists in `crates/nif/src/import/mesh/tangent_convention_tests.rs`; there is no Z-up sibling of it.
- **Impact**: For a degenerate vertex whose normal is near `(k,k,k)` the stored `Vertex.tangent.xyz` is parallel to `N`. That value clears `perturbNormal`'s `dot(T,T) > 1e-4` Path-1 gate, and Path 1's un-guarded Gram-Schmidt (`normalize(T - dot(T,N)*N)`, see REN-D19-04) then evaluates `normalize(vec3(0))` → **NaN in the shaded normal**. Trigger is narrow: the degenerate arm only fires for vertices whose adjacent triangles all have zero UV area, or that no triangle references. Reached by every Z-up producer — `ni_tri_shape.rs` (both `NiTriShapeData` and de-stripped `NiTriStripsData`) and `bs_tri_shape.rs`'s third tangent branch — i.e. Oblivion / FO3 / FNV interior content, the largest corpus in the project.
- **Related**: #2632 (the Y-up fix); REN-D19-04 (the shader-side guard that would contain it); `bitangent_sign` / #1516 and `clamp_sign` / #2313 (both intact).
- **Suggested Fix**: Port the `synthesize_tangents_yup` degenerate arm verbatim into `synthesize_tangents` (project the permuted vector against `n_yup`, `normalize_inplace`, then cross), and add the Z-up sibling test to `tangent_convention_tests.rs`.

#### REN-D20-01: egui-winit's raw-input event queue grows without bound while the overlay is hidden

- **Severity**: MEDIUM
- **Dimension**: 20 — Debug/Telemetry
- **Location**: `crates/debug-ui/src/lib.rs` (`DebugUiState::run`, `DebugUiState::on_window_event`); forwarding site `byroredux/src/main.rs` (`window_event`)
- **Status**: NEW
- **Description**: `DebugUiState::on_window_event` is invoked for **every** `winit::WindowEvent`, unconditionally — the binary's `window_event` calls it before any visibility check (the `egui_consumed` binding is computed first, then used only to decide whether to *skip* the camera layer). `egui_winit::State::on_window_event` appends translated events onto its private `egui_input.events` `Vec`, which is drained **only** by `take_egui_input`. `DebugUiState::run` is the sole caller of `take_egui_input`, and it short-circuits before reaching it:

  ```rust
  if !self.visible && snapshot.interaction_prompt.is_none() {
      return PanelOutputs::default();
  }
  let raw_input = self.egui_winit.take_egui_input(window);
  ```

  `visible` is `false` at boot, and `interaction_prompt` is `None` except when the player is aimed at an activatable reference. So in the default configuration — overlay closed, nothing under the crosshair — **nothing ever drains the queue**.
- **Evidence**: `byroredux/src/main.rs`'s `window_event` calls `state.on_window_event(win, &event).consumed` for all events with no `visible` gate. In `egui-winit-0.33.3`, `on_window_event` pushes into `self.egui_input.events` on the cursor-moved, mouse-wheel, pointer-button, key, touch and cut/copy/paste arms, and `take_egui_input` ends in `self.egui_input.take()` — the only drain. `take_egui_input` appears exactly once in `crates/debug-ui/src/lib.rs`, after the early return.
- **Impact**: One `egui::Event` retained per forwarded mouse-move / key / wheel / touch event, **for the lifetime of the process**, in host RAM. A fly-camera session produces `CursorMoved` continuously, so the queue grows monotonically for as long as the operator never opens the overlay — which is the expected steady state, since the overlay is opt-in behind F3. Second-order: the first F3 press hands egui the entire accumulated backlog in a single `RawInput`, so that frame replays every queued pointer/key/paste event at once (one-shot hitch plus nonsense interaction state). Recovered only by opening the overlay.
- **Related**: #2166 (per-system tracker armed on first overlay open — same "hidden overlay is the steady state" assumption); #2247 (`merge_egui_pending_output`, the mirror-image "skipped egui frame drops state" bug on the renderer side).
- **Suggested Fix**: On the short-circuit branch of `run`, still drain and discard: `let _ = self.egui_winit.take_egui_input(window);` before returning. That keeps egui-winit's viewport/modifier bookkeeping current so the first visible frame is correct. Gating the `on_window_event` forwarding on `visible` instead is the **worse** fix — egui would then miss modifier/focus state across the toggle boundary.

#### REN-D23-01: `view_space_to_meters_factor` is hard-coded to `1.0`, but the engine's view space is Bethesda units (70 per metre)

- **Severity**: MEDIUM
- **Dimension**: 23 — FSR Upscaler (SDK input contract)
- **Location**: `crates/renderer/src/vulkan/frame_upscaler.rs` — the `view_space_to_meters_factor: 1.0` field of the `fsr3::DispatchDescription` literal inside `FrameUpscaler::record`
- **Status**: NEW
- **Description**: FSR derives view-space depth from the `camera_near` / `camera_far` / `camera_fov_angle_vertical` triple the engine supplies, then converts it to metres by multiplying by `viewSpaceToMetersFactor`. `build_fsr_frame_parameters` (`crates/renderer/src/vulkan/context/draw.rs`) sources that triple from `Camera::near` / `Camera::far` via `DofView`, i.e. **Bethesda units** — the whole renderer treats world/view units as BU, which is exactly why `crates/renderer/src/vulkan/volumetrics.rs` defines `WORLD_UNITS_PER_METER = byroredux_core::lighting::BETHESDA_UNITS_PER_METER` (= 70.0) and divides by it. The dispatch nevertheless declares one view-space unit == one metre, so every "metres" quantity inside the SDK is inflated 70×. The parameter appears nowhere in `docs/engine/fsr3-upscaler-integration-plan.md`'s input-contract section — an unexamined default, not a considered choice.
- **Evidence**: All SDK consumers go through `GetViewSpaceDepthInMeters(d) = GetViewSpaceDepth(d) * ViewSpaceToMetersFactor()`, and two are distance-tuned. `ReconstructedDepthMvPxThreshold(m) = ffxLerp(0.25f, 0.75f, ffxSaturate(m / 100.0f))` is intended to ramp over 0–100 m; fed BU it saturates at the far-field 0.75 px past 100 BU ≈ **1.43 m**, so effectively the entire frame uses the far-field motion-vector threshold. `const FfxFloat32 fDistanceFactor = ffxSaturate(0.75f - params.fFarthestDepthInMeters / 20.0f);` is zero for anything past 15 BU ≈ **0.21 m**, so that term is dead across the whole scene — and it is one of the `max` inputs to the history-rectification box scale, so near geometry never gets the tighter, more history-rejecting clipping box AMD tuned for it. Secondary: `prepare_inputs.h` clamps with `ffxMin(GetViewSpaceDepthInMeters(...), FSR3UPSCALER_FP16_MAX)`, which at factor 1.0 saturates at 65 504 BU ≈ 936 m against a `Camera::far` of 300 000 BU, so exterior far-field depth also flat-tops; at the correct factor the same range maps to ≈ 4285 m, comfortably inside FP16.
- **Impact**: Reconstruction runs with two of its distance-dependent heuristics permanently pinned to their far-field values on every scene, at every preset, **on the engine's default render path** (`UpscalerMode::default()` is `Fsr3(Quality)`). Visual only — expected signature is extra history retention (mild ghosting/smearing) on near-camera surfaces, and small sub-pixel motions discarded during depth reconstruction. Invisible to `cargo test`, to the validation layers, and to the SSIM matrix in `byroredux/tests/upscaler_quality.rs`, which scores FSR against the engine's own TAA render rather than ground truth.
- **Related**: `crates/renderer/src/vulkan/volumetrics.rs` (the one subsystem that already does the BU→m conversion correctly); `docs/engine/fsr3-upscaler-integration-plan.md`; REN-D23-06 (same "SDK contract asserted by hand rather than queried" class).
- **Suggested Fix**: Pass `1.0 / byroredux_core::lighting::BETHESDA_UNITS_PER_METER` sourced from the existing constant rather than a new literal, and add the parameter to the plan's input-contract table. Re-run the quality matrix afterwards — **a shift in the committed thresholds is the measurement of the fix, not a regression.** (Blocked in practice on REN-D23-02: there is currently no working bench.)

#### REN-D23-02: the FSR bench harness changed its measurement conditions *and* its TSV schema in `f19f7f15` without a re-bench, and `fsr_bench_report.py` now crashes on its own committed archive

- **Severity**: MEDIUM
- **Dimension**: 23 — FSR Upscaler (bench-harness stability)
- **Location**: `scripts/fsr-bench-matrix.sh` (the `--bench-mode renderer-stepped --bench-camera "$CAMERA_PATH"` addition and the widened `printf` header) and `scripts/fsr_bench_report.py` (`main`), against `docs/audits/BENCH_R6a-stale-17_head_3a02b02d.tsv` and `docs/audits/BENCH_R6a-stale-17_control_e153b50c.tsv`
- **Status**: NEW
- **Description**: `git log --oneline -- scripts/fsr-bench-matrix.sh scripts/fsr_bench_report.py` returns exactly two commits: `e153b50c` (2026-07-24) and `f19f7f15` (2026-08-11). That second commit changed three things at once: (a) every run now executes `--bench-mode renderer-stepped --bench-camera "$CAMERA_PATH"` instead of the previous parked-camera capture, so **the workload being measured is different**; (b) the TSV gained six columns (`mode`, `camera`, `sim_time_s`, `lights`, `tlas`, `state_hash`); (c) the report script gained a scene-state fingerprint gate that indexes those columns unconditionally. **No bench table was refreshed in or after `f19f7f15`** — `git log f19f7f15..HEAD -- ROADMAP.md docs/engine/fsr3-upscaler-integration-plan.md docs/audits/BENCH_*.tsv` returns only the session-65 closeout, which touched neither. Every published FSR number therefore describes the pre-`f19f7f15` harness and cannot be compared against any run of the current one.
- **Evidence**: reproduced on the committed archive:
  ```
  $ python3 scripts/fsr_bench_report.py docs/audits/BENCH_R6a-stale-17_head_3a02b02d.tsv
    File ".../scripts/fsr_bench_report.py", line 102, in main
      row["mode"],
  KeyError: 'mode'
  ```
  Both committed TSVs still carry the 17-column pre-`f19f7f15` header while `fsr-bench-matrix.sh` now emits 23 columns.
- **Impact**: The two artefacts the repo keeps specifically so cross-commit FSR comparisons stay checkable are **unreadable by the tool that produced them**, and the phase-7 net-frame-recovery table — the stated justification for FSR Quality being the engine default — has no reproducible path. The methodology change is itself defensible (`docs/engine/fsr3-troubleshooting.md` argues a parked camera hides disocclusion failures, and `f19f7f15` did update that doc), but it landed without re-taking the baseline it invalidates. ROADMAP.md independently flags the bench-of-record as 116 commits stale and "unreliable", so **there is currently no live FSR bench of any kind** — which blocks measuring REN-D23-01's fix.
- **Related**: #2560, #2084, #2279 (all closed, same bench-staleness class); ROADMAP.md R6a-stale-19.
- **Suggested Fix**: Re-run `scripts/fsr-bench-matrix.sh` on a current HEAD and replace the phase-7 table with the stepped-camera figures, labelling the old table with the harness commit it was taken on. Make `fsr_bench_report.py` tolerate a missing column (`row.get(key, "-")`) so the committed historical TSVs stay readable, or archive them with an explicit harness-commit header line. **No FPS or ms figure is asserted anywhere in this report; that is the point of the finding.**

---

### LOW (62)

Compact form to keep the report readable. **Every LOW from every dimension is
here — none was dropped.** Full detail for each lives in its dimension scratch
file at `/tmp/audit/renderer/dim_N.md` under the same ID. All are **NEW** unless
the Status column says otherwise.

| ID | Dim | Location | Finding | Status |
|---|---|---|---|---|
| REN-D1-02 | 1 | `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas`) | A second full pass over `draw_commands` re-bumps LRU stamps `build_tlas_instances` already set; the two passes are not equivalent — the second also protects BLAS the first dropped on the `missing_ssbo_instance` arm. Predates the #2259 split. | NEW |
| REN-D1-03 | 1 | `crates/renderer/src/vulkan/acceleration/predicates.rs`, `crates/renderer/src/vulkan/context/draw.rs`, `crates/renderer/src/vulkan/acceleration/tests.rs`, `crates/renderer/shaders/triangle.frag` | Magic material kind `11` (MultiLayerParallax) hand-copied at four live sites, one gating the TLAS instance shadow mask; `MATERIAL_KIND_GLASS` beside it is imported from the shared table. Test declares a 4th copy, so it cannot detect `is_refractive_glass` drifting. | NEW |
| REN-D1-04 | 1 | `crates/renderer/src/deferred_destroy.rs`, `crates/renderer/src/vulkan/skin_compute.rs`, `crates/renderer/src/vulkan/context/draw.rs` | Three live comments cite the pre-Session-35 `acceleration.rs` monolith; one names `tick_pending_destroy_blas` (no such symbol — it is `tick_deferred_destroy`), one uses a rotted `draw.rs:889` anchor now pointing at a DOF test. | NEW |
| REN-D1-05 | 1 | `crates/renderer/src/vulkan/acceleration/memory.rs` (`shrink_tlas_scratch_to_fit`, case 2) | The live-slot realloc arm appears unreachable — `current` and `peak` are written together in `ensure_tlas_state` and differ by ≤ `scratch_align − 1`, so `current > 2 × peak` cannot hold. All reclamation flows through case 1. Unit tests on the predicate give false confidence #1226 revived it. Confirm with a one-shot `log::debug!` before touching; **do not** change the shrink/destroy ordering (that is the #1782-class safety property). | NEW |
| REN-D2-01 | 2 | `crates/renderer/shaders/triangle.frag` (reservoir write vs. `spatialDepthCompatible` read) | ReSTIR spatial reuse is provably inert past ~66 841 BU: the depth lane is clamped to 65504 on write, compared against unclamped `worldDist` on read. Five wasted reservoir fetches per pixel there. **Cluster D-2.** | NEW |
| REN-D2-02 | 2 | `crates/renderer/shaders/triangle.frag` (`RESERVOIR_LIGHT_MASK`) vs. `crates/renderer/src/vulkan/scene_buffer/constants.rs` (`MAX_LIGHTS`) | The 10-bit light lane has no lockstep guard, and the two constants are structurally unable to see each other (GLSL literal vs. `pub(super)`). Correct today only because 511 < 1023; raising `MAX_LIGHTS` silently selects the **wrong light**. | NEW |
| REN-D2-03 | 2 | `docs/engine/shader-pipeline.md` (Set-1 table) | Binding 11 still described as a bare `u32` (it has been an 8-word `GpuRayBudget` since `5798e467`, so a range/flush/barrier sized from the row is wrong by 28 B); bindings 8/9/13 listed triangle-only though `water.frag` statically reads all three. | NEW — **merged with stale-run `REN-D2-01`, file once** |
| REN-D4-02 | 4 | `crates/renderer/src/vulkan/sync.rs` | The per-swapchain-image `render_finished` contract (`548c1b69`, VUID-…-00067) is prose-only — 6 grep hits, none in a `#[cfg(test)]` block. | NEW (carried from the 11:45 pass, re-verified) |
| REN-D4-03 | 4 | `crates/renderer/src/vulkan/egui_pass.rs` | The `in_dep` comment says it chains after composite's swapchain write and that composite's outgoing dep sets `dstStage = NONE`; since the FSR tail, `PresentationPipeline` writes the swapchain and its outgoing dep uses `COLOR_ATTACHMENT_OUTPUT \| TRANSFER`. | NEW (carried, re-verified) |
| REN-D4-06 | 4 | `docs/engine/shader-pipeline.md` (§ Per-Frame Submission Order) | The authoritative 22-step order omits `copy_depth_to_history` (a whole `TRANSFER`-stage step between 5 and 6 that transitions the depth image twice) and the step-21 health-counter harvest. `depth_history_image` is absent from the doc entirely, including the G-Buffer table — and **#2484 and #2485 are open findings about exactly that barrier and that image**. | NEW |
| REN-D5-04 | 5 | `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`allocate_scene_render_buffers`) | The bind-inverse staging comment computes "16 × 144 × 64 ≈ 144 KB"; the constant is 1366, making it **≈ 12.6 MB** — an 87× understatement next to the second-largest host-visible allocation the renderer makes. `constants.rs` and `memory-budget.md` are both correct; only this site is stale. | NEW |
| REN-D5-05 | 5 | `crates/renderer/src/vulkan/context/resources.rs` (`collect_image_health`), `crates/renderer/src/vulkan/buffer.rs` | The health counter is read and rewritten through `mapped_slice_mut` with neither invalidate nor the flush `mapped_slice_mut`'s own doc mandates; `GpuBuffer` has no invalidate primitive at all. Benign **only** because gpu-allocator 0.27 puts `HOST_COHERENT` in the *required* flag set for `CpuToGpu` — and nothing in the source says so. | NEW (documentation half of REN-D4-04 / REN-D4-05) |
| REN-D5-06 | 5 | `crates/renderer/src/deferred_destroy.rs` | Module doc claims "two production users"; there are three — `pending_destroy_scratch` (#1782's fix) is omitted, so a reader auditing deferred-destroy coverage concludes the shared BLAS scratch is *not* on the countdown path, which is the exact wrong conclusion that produced #1782. Both `DEFAULT_COUNTDOWN` cross-references are rotted. | NEW |
| REN-D7-2026-08-12-01 | 7 | `byroredux/src/main.rs` vs. `crates/renderer/src/vulkan/material.rs` + `docs/engine/memory-budget.md` | Three authorities describe the `MAX_MATERIALS` over-cap path as a supported degrade (id 0 + warn-once); `main.rs` then `debug_assert_eq!`s the overflow count is zero, so a plain `cargo run` on a large/modded exterior **panics** where the degrade is already implemented, tested and documented. The same doc records the opposite call for `MAX_INSTANCES` (#956/#992). Reachable per the code's own recorded Skyrim radius-3 measurement (4000+ unique materials). Debug builds only. | NEW |
| REN-D7-2026-08-12-02 | 7 | `crates/renderer/shaders/ui.vert`, `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`, `crates/renderer/src/vulkan/scene_buffer/descriptors.rs` | The UI slot-0 comment argues from the pre-#807 premise ("`materials[0]` is the FIRST scene material"), contradicting `gpu_types.rs`'s corrected contract — and slot-0 semantics is what the over-cap fallback rests on. Cites `scene_buffer.rs:172-176`; `scene_buffer` has been a **directory** since Session 34/35. `material.rs:281-294` is likewise rotted (`as_bytes` is near 594). | NEW |
| REN-D8-NEW-02 | 8 | `crates/renderer/src/vulkan/context/post_passes.rs` (`record_ssao_pass` doc) | Doc says AO is "current-frame (no lag)" because SSAO runs before composite — but `composite.frag` has no AO binding at all; the sole reader is `triangle.frag` in the **main render pass**, which runs earlier. With per-FIF AO images the sampled AO is **two frames old**, not zero and not one. `triangle.frag`'s own "computed last frame" is closer but still off by one slot. | NEW |
| REN-D8-NEW-03 | 8 | `crates/renderer/src/vulkan/composite.rs` | The `composite_dep_in` comment calls attachment 0 "the swapchain image"; it is `scene_image_views[i]`, an offscreen `HDR_FORMAT` image. The dependency's *reasoning* is still correct — only the noun is stale — and the module docstring at the top of the same file is already right, so the file contradicts itself. | NEW |
| REN-D8-NEW-04 | 8 | `crates/renderer/src/vulkan/svgf.rs` | `svgf_temporal_clamps_fireflies_before_history_branch`'s doc names a TAA sibling test (`taa_comp_floors_alpha_for_moving_pixels_under_parked_camera`) added by `c6342845` and removed by `e5d02f83` — a dead symbol in the doc of a regression guard whose whole purpose is surviving refactors. Live nearest sibling: `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces`. | NEW |
| REN-D9-2026-08-12-03 | 9 | `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (`record_skinned_blas_refit`) | `failed_skin_slots` (#900) gates slot *allocation* only; a failed `build_skinned_blas_batched_on_cmd` records nothing, so the entity re-runs the size query + allocation every frame and logs **two WARNs per entity per frame** (build-failure + the refit that then cannot find the BLAS), while `refits_attempted` counts attempts that could never succeed. Fires precisely under the VRAM pressure that caused it. | NEW |
| REN-D9-2026-08-12-04 | 9 | `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` | `_skin_chain_ns` is dead telemetry — measured every frame since M29 (`1ae235b9`), never consumed anywhere. Now load-bearing as a **source-position anchor** for the #2494 regression test, so naive deletion breaks `skin_eviction_runs_without_global_vertex_buffer_tests`. The CPU-side skin-chain wall time is genuinely unmeasured. | NEW |
| REN-D9-2026-08-12-05 | 9 | `docs/engine/shader-pipeline.md` | (a) Compute table says `skin_vertices.comp` deforms "positions **/ normals**"; it has been position-only since #2170 (`SKIN_OUTPUT_STRIDE_FLOATS = 3`), and the shader body says so — so the doc contradicts the code's own explanation of a live behavioural gap. (b) `MAX_TOTAL_BONES` factorised as 144 × 1364 = 196 416 ≠ 196 608, omitting the reserved identity slot 0. | NEW |
| REN-D10-03 | 10 | `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera::render_origin`, `::position`) | The "Consumers (#1492)" list credits `triangle.vert` with the absolute reconstruction; since #1496 that moved to `triangle.frag`, which the list omits entirely despite now being the busiest consumer (absolute reconstruction, `camRel` soft-particle rebase, `renderOrigin.w` FSR-reset view). `position` is documented "xyz = world position" where the rest of the block carefully distinguishes frames — it is ABSOLUTE. | NEW |
| REN-D10-04 | 10 | `docs/engine/shader-pipeline.md` (§ Absolute world space — f32 ceiling) | Cites *cell_loader/references.rs* (as written in the doc) for the `RT_ABSOLUTE_PRECISION_CEILING` guard; `byroredux/src/cell_loader/references/` is a **directory** — the constant and predicate are in `byroredux/src/cell_loader/references/mod.rs`, the firing `debug_assert!` in `byroredux/src/cell_loader/references/complete.rs`. Everything else in the section is accurate. | NEW |
| REN-D10-05 | 10 | `crates/renderer/shaders/ssao.comp`, `crates/renderer/src/vulkan/context/post_passes.rs`, `crates/renderer/shaders/triangle.frag` | `ssao.comp` declares `cameraPos` as "camera world position" while the host deliberately feeds `ssao_cam_rel = camera_pos − render_origin`; the shader math is correct (all uses are differences), the comment is not — and it is what a future author reads before adding an absolute-space consumer. Both this rebase and #1642's soft-particle `camRel` are **unpinned**, unlike the four siblings that all got static source-check tests. | NEW |
| REN-D11-2026-08-12-03 | 11 | `crates/renderer/src/vulkan/context/helpers.rs` (`create_render_pass`, attachment 3) | Comment pins `triangle.frag:1532` for the bit-31 flag; that line is inside the glass Fresnel block, ~1000 lines from the real `outMeshID` write. A live example of exactly the rot the symbol-anchor rule exists to prevent — in the file the Dim-11 checklist names as its entry point, about the contract that just silently drifted (Cluster A/B). | NEW |
| REN-D11-2026-08-12-04 | 11 | `crates/renderer/shaders/triangle.frag` (`layout(location = 3) out uint outMeshID;`) | Trailing comment still reads "per-instance ID + 1", the pre-`883f57cd` meaning for all draws. `gbuffer.rs` and `shader-pipeline.md` were both updated; the shader that actually *writes* the value was not — the single most load-bearing declaration of the two-meaning encoding. | NEW (third site of the #2499/#2500 drift class) |
| REN-D11-2026-08-12-05 | 11 | `crates/renderer/src/vulkan/pipeline.rs` (UI pipeline builder) | Comment sizes `Vertex` at 100 bytes; it has been **104** since the vertex colour widened `vec3 → vec4` (`cd2b5fe4`), and `vertex_size_matches_attribute_stride` asserts 104. `vertex.rs`'s own `UiVertex` doc already says 104. | NEW |
| REN-D11-2026-08-12-06 | 11 | `crates/renderer/src/vulkan/gbuffer.rs` (module header) | Opens "Five auxiliary render targets" while the table two lines below correctly lists **seven** (the FSR reactive + transparency attachments, `5c56e311` / `5c7acfe2`). | NEW — **merged with stale-run `REN-D11-02`, file once** |
| REN-D11-2026-08-12-07 | 11 | `crates/renderer/shaders/water.vert` (comment above `struct GpuInstance`) | Claims a "112-byte invariant" pinned by `gpu_instance_is_112_bytes_std430_compatible`; the struct body is **correct** (128 B, byte-identical to the other four mirrors) and the live test is `gpu_instance_is_128_bytes_std430_compatible` — no `_112_` symbol exists in the tree. `water.vert` is the only mirror still carrying the stale size claim, and historically the most likely to drift (#1498 had to add it to the name-drift guard list). | NEW — **three-way merge: Dim 11 `-07`, Dim 15 `REN-D15-06`, stale-run `REN-D11-03`. File once.** |
| REN-D12-2026-08-12-02 | 12 | `crates/renderer/src/vulkan/context/draw.rs` (`group_state`, `needs_two_sided_blend_split`) | `order_dependent_glass` is computed for **every** batch from `is_refractive_glass`, which accepts opaque MultiLayerParallax; `group_state` carries it unconditionally, so an *opaque* MLP batch gets a distinct merge key and fragments an otherwise-homogeneous run into three indirect groups — where the split it protects can never apply (`needs_two_sided_blend_split` requires `is_blend`). Draw-call-count noise only; zero on games with no such content. | NEW |
| REN-D12-2026-08-12-03 | 12 | `crates/renderer/src/vulkan/context/geometry_pass.rs` (`dispatch_direct` + its three call sites) | `indirect_call_count` is incremented (`+= 2` on the split branch, `+= 1` otherwise) even when `dispatch_direct` returned early without recording — missing mesh, or no per-mesh buffers (the #1370 global-only distant-LOD case). Makes the post-batch GPU-draw metric an upper bound rather than the actual count, on the `global_bound == false` path only. | NEW |
| REN-D13-03 | 13 | `crates/renderer/src/vulkan/taa.rs` (`TaaPipeline::dispatch`) | Hard-codes `div_ceil(8)` instead of the generated `WORKGROUP_X`/`WORKGROUP_Y` that `taa.comp`'s `local_size` is built from. Lowering the tile would leave the bottom-right of the TAA output **never written** (that slot's history retains the previous cycle's contents, which composite then samples as this frame's HDR). `bloom.rs` and `volumetrics.rs` already import the constants; `taa.rs`, `svgf.rs`, `ssao.rs` and `caustic.rs` still use the literal. | NEW |
| REN-D13-04 | 13 | `crates/renderer/src/vulkan/taa.rs` (module const-assert + `write_descriptor_sets`) | The local `MAX_FRAMES_IN_FLIGHT >= 2` assert is weaker than the `(f + 1) % MAX_FRAMES_IN_FLIGHT` history arithmetic requires — that expression selects the previous slot **only at exactly 2**. At 3 it resolves to two frames ago. The real gate is `sync.rs`'s `== 2` (#870), whose own comment enumerates the two remedies that would allow relaxing it, so relaxing it is a contemplated change. `volumetrics.rs` already uses the general form. `svgf.rs` has the same shape and the same weak reasoning. | NEW |
| REN-D13-05 | 13 | `crates/renderer/src/vulkan/context/draw.rs` (`taa_jitter`), `crates/renderer/src/vulkan/upscaling.rs` (`fsr_pixel_jitter_to_ndc`), `crates/renderer/shaders/triangle.frag` | TAA and FSR write `GpuCamera.jitter.xy` under **opposite Y-sign conventions** through the same match arm. The applying consumers are sign-agnostic, so this is not a rendering bug — but `triangle.frag`'s `DBG_VIZ_FSR_TEMPORAL` branch *inverts* the mapping and hard-codes the FSR convention with no gate on the active upscaler, so that view is sign-wrong in TAA mode. An unlabelled trap for any future analytic jitter cancellation. | NEW |
| REN-D14-NEW-03 | 14 | `crates/renderer/src/vulkan/caustic.rs` (`CausticParams` in `dispatch`) | Nothing pins that the uploaded `tune.x` equals the `CAUSTIC_FIXED_SCALE` composite divides by — the value travels by two channels (runtime UBO lane vs. compile-time `#define`) and the comment claiming otherwise names two `shader_constants` tests that check neither. The composite side *is* pinned; the **upload** side is the unpinned link. Failure mode if `tune.x` ever becomes a live tunable: silent global brightness error on every caustic pixel. | NEW |
| REN-D14-NEW-04 | 14 | `crates/renderer/src/vulkan/caustic.rs` (`CausticSlot::sampled_view`, `create_slot`) | `storage_view` and `sampled_view` are built from the same closure with identical `ImageViewCreateInfo` — byte-identical handles — while the field doc claims a distinction the `610cb170` RGB-array refactor removed. Four redundant `VkImageView`s where two suffice, and every teardown path has to destroy both. | NEW |
| REN-D14-NEW-05 | 14 | `crates/renderer/src/vulkan/caustic.rs` (`clear_for_skip`), `crates/renderer/src/vulkan/context/post_passes.rs` | `clear_for_skip` zeroes all three layers but leaves `parked_frames[frame]` untouched (`advance_parked_visits` runs only inside `dispatch`), so a skip streak under a bit-identical view-proj resumes at a near-cap decay against an empty pool — at the `CAUSTIC_DECAY_MAX = 0.995` ceiling, a 0.005 new-sample weight, fading back in over ~200 slot-visits. Narrowly reachable (any camera motion resets it); one-line fix. | NEW |
| REN-D15-03 | 15 | `crates/renderer/shaders/water.frag` (the `uv01` guard and `ivec2 pixel` conversion) | The screen-space guard is inclusive on the upper edge (`lessThanEqual(uv01, vec2(1.0))`), so at `uv01.x == 1.0` exactly, `pixel.x == screen.x` — one past the last valid texel. Benign (Vulkan discards out-of-range image writes) but it is the same conversion that runs wholesale against the 1×1 `placeholder_caustic_sink` fallback, relying on that robustness rule. `caustic_splat.comp` rejects explicitly against `size`. | NEW |
| REN-D15-04 | 15 | `byroredux/src/env_translate.rs` → `crates/renderer/src/vulkan/water.rs` (`WaterPush::shallow.a`) → `crates/renderer/shaders/water.frag` | `WaterMaterial::fog_near` travels the whole EXAL water arm and **nothing reads it**. Absorption is keyed exclusively on `fog_far` through a hard-coded `exp(-2t)` curve identical for every water body in every game, so authored per-WATR clarity is ignored. The dead slot sits in a block whose own doc says 128 B is "exactly the Vulkan 1.1 spec minimum … no further growth is possible", so a WATAL §5.1 promotion will have to displace something. Same shape as the already-fixed `wave_amplitude` gap (#1936/#1969). | NEW |
| REN-D15-05 | 15 | `crates/renderer/shaders/water.frag` (`ampScale` / `freqScale` divisors), `crates/core/src/ecs/components/water.rs`, `crates/plugin/src/esm/records/misc/water.rs`, `byroredux/src/render/water_wave_params_tests.rs` | The #2240 normalisation sentinels (`0.05`, `0.6`) are hard-coded literals in the shader and duplicated in two Rust `Default` impls with no lockstep guard — though the `WATER_CALM…WATERFALL` enum values used by the *same shader* already go through `shader_constants_data.rs` + the #1780 include test. Worse, the test named for the contract (`default_wave_params_are_the_sentinel_the_shader_normalises_against`) passes the values in **explicitly** rather than reading `WaterMaterial::default()` — a pass-through tautology. | NEW |
| REN-D15-08 | 15 | `docs/engine/watal.md` §2 | (a) Lists the #1502 procedural-noise banding as a *current* fragility; it is fixed — `sampleScrollingNormal` and `foamFlowStreaks` both subtract `originOffset` before hashing, and the textured branch stays absolute *deliberately* with the #2496 texel-integral bound. The Dim-15 brief asks that #1502 be recast as a regression guard; the doc contradicts that and invites a re-fix that would re-break the deliberate absolute-UV branch. (b) Cites `resolve_water_material` at `env_translate.rs:89-176`; the function is near line 352. | NEW |
| REN-D15-09 | 15 | `byroredux/src/systems/water.rs` (`submersion_system`) | Two "no water data" exits with opposite behaviour: the `WaterPlane`-absent exit resets `SubmersionState` to default with a comment explaining why; the `WaterVolume`-absent exit twenty lines later is a bare `return`, so the camera keeps `head_submerged: true` and a stale `material` that `compute_underwater_params` then feeds indefinitely. Only separable via `spawn_lod_water_plane` (#2449), which inserts `WaterPlane` without `WaterVolume` — a state in which the camera cannot already be submerged, so today's outcome is "no reset needed". Defence-in-depth gap. | NEW (independently re-derived; also in the quarantined 11:44 scratch) |
| REN-D15-10 | 15 | `crates/renderer/shaders/water.frag` (`PI`, `SHORELINE_RAY_MAX`, the `reflColor` `mix`) | Three dead items. `SHORELINE_RAY_MAX = 256.0` is the misleading one — it reads as the cap on `foamShoreline`'s ray, but that function's `tMax` is `push.tune.z` (`shoreline_width`, default 32.0, never overwritten), so the 256 has no effect and contradicts the live value by 8×. The `mix(reflectionMiss, reflColor, reflHit ? 1.0 : 0.0)` is a provable no-op (`traceWaterRay` already returns `missFallback == reflectionMiss` when `reflHit` is false). All fold away; the cost is reviewer time. | NEW |
| REN-D16-06 | 16 | `shader_constants_data.rs`, `crates/renderer/src/vulkan/bloom.rs`, `crates/renderer/shaders/bloom_upsample.comp`, `crates/renderer/shaders/bloom_downsample.comp` | `BLOOM_INTENSITY = 0.15` carries **two mutually exclusive documented derivations** — one says it absorbs the un-normalised 5× DC gain relative to Frostbite's 0.04, the other says it compensates LDR-authored Bethesda content; the 4× factor is spent once in each comment on a different justification, and absorbing a 5× gain against 0.04 would require ≈ 0.008. Measurable consequence: the effective DC weight is **0.75×** the local blurred average (~19× the normalised reference), and `bloom_downsample.comp` applies **no bright-pass threshold or Karis average** (`DownsampleParams` carries only `inv_resolutions`), so this is a broadband lift, not a highlight-only glow. Filed as a documentation contradiction plus a quantified observation — **not** a claim the image is wrong. | NEW |
| REN-D17-06 | 17 | `crates/renderer/shaders/include/pbr.glsl` (`specularAaRoughness`) | The `#2471` doc claims parity with Kaplanyan & Hoffman 2016 / Filament `normalFiltering()`, but two constants in the same expression are not from that reference and carry no citation: the bare `0.25` variance coefficient (Filament uses a *named* `SPECULAR_AA_VARIANCE`, default 0.15), and the **missing** `SPECULAR_AA_THRESHOLD` cap on the *added* kernel term — this shader clamps only the sum, so a high-frequency normal (foliage cutouts, chain-link, fine grating) can drive a polished surface to `roughness = 1.0` in one step, which the reference filter explicitly prevents. `grep -rn "SPECULAR_AA"` → no hits. Every neighbouring constant in the file *is* cited. Propagates into the anisotropic lobe via `deriveAxAy`. **Do not tune blind** (§7). | NEW |
| REN-D17-07 | 17 | `crates/renderer/shaders/triangle.frag` (the ~60-line F0 comment block) | Still documents, in the present tense, the spec-colour-as-F0 branch that `31c99bb3` deleted ("So for PBR materials we use the authored spec_color as F0 directly"), reversing course only in its final third. There is no such branch: `F0` is assigned exactly twice, both `f0Dielectric`-derived. The stale half also contradicts the live CPU contract described by #2703, so a reader trusting it looks for the bug in the wrong layer — on the single largest comment block in the F0 region, which is where someone goes to ask "why does my FO4 metal panel look plastic". | NEW |
| REN-D17-08 | 17 | `crates/renderer/shaders/include/pbr.glsl` (`distributionGGXAniso`, `deriveAxAy`) | The #1250 isotropic-degeneracy contract and the #1254 anisotropic clamp have **zero** automated guards, unlike every sibling invariant in this dimension (#2243, #2244, #2472, #1190 all have string-mirror tests with negative assertions). `grep -rn "distributionGGXAniso\|deriveAxAy" --include=*.rs` → nothing. Both contracts were verified **algebraically** to hold today; the exposure is purely regression, in a lobe with no CPU producer, so a break would not be caught by eyeball either. | NEW |
| REN-D17-09 | 17 | `crates/renderer/src/vulkan/material.rs` (`pub mod presets`) | (a) The module pins its values to `knightcrawler25/GLSL-PathTracer`, which the user-memory note *reference_glsl_pathtracer.md* records as cloned to `/mnt/data/src/reference/` — **it is not there**, so the Dim-17 checklist item "Disney preset constructors match documented values (cross-ref GLSL-PathTracer)" is not executable offline and every preset scalar is citable but unverifiable. Same for the four `pbr.glsl` doc references into `disney.glsl` line ranges. (b) The doc claims the presets are the "fallback when authored BGSM is absent"; **no such fallback exists** — `translate_material` never consults `presets`, and the only hits outside `material.rs` are its own tests. A documented fallback role no code implements is an invitation to wire it in and bypass the NIFAL single boundary. | NEW |
| REN-D18-04 | 18 | `byroredux/src/env_translate.rs` (`climate_tod_hours`'s `FALLBACK`, `procedural_fallback_weather`), `byroredux/src/systems/weather.rs` (`DEFAULT_TOD_HOURS`) | The no-climate TOD quad `[6, 10, 18, 22]` is written out independently in three places, consumed by different producers of the *same* "exterior, no authored climate" state — violating the rule `apply_neutral_exterior_fallback`'s own doc records ("the **one** canonical EXAL boundary fallback … not a private set", the #1722 lesson). `DEFAULT_TOD_HOURS`'s doc even *asserts* the coupling while referencing neither of the other two literals. A future re-anchor applied to one or two silently splits the fallback sun arc from the fallback palette interpolation. | NEW |
| REN-D18-05 | 18 | `byroredux/src/systems/weather.rs` (`compute_sun_arc`), `byroredux/src/env_translate.rs` (`SUN_INTENSITY`), `byroredux/src/render/mod.rs` (`SUN_INTENSITY_PEAK`) | Peak sun intensity `4.0` is spelled independently in the producer, the bootstrap seed, and the divisor it is normalised by — three unrelated declarations in different modules that must be equal for the exterior directional ramp to span `[0, 1]`. Both sun-arc tests assert against their own hardcoded `4.0`/`3.6`, so they stay green through a one-sided change. Raising the producer alone saturates the ramp early; lowering it alone caps daytime exterior directional below full strength. Whole-frame exterior lighting, silently. | NEW |
| REN-D18-06 | 18 | `byroredux/src/env_translate.rs` (`resolve_worldspace_climate` vs. `inherit_up_chain`) | `e681a3c1` introduced the generic cycle-guarded `inherit_up_chain` and routed DNAM / NAM3+NAM4 / NAM2 through it; **CNAM (climate)** — the one PNAM bit this dimension depends on — kept its bespoke pre-helper loop, duplicating the `visited` cycle guard, the linear form_id reverse lookup, the three `warn!` termination cases and the precedence ordering. Reachable through the generic helper unchanged. A future fix to the shared walk lands in one copy and silently misses climate — the highest-traffic bit, since a missed climate downgrades a whole worldspace to the procedural Mojave fallback sky. | NEW |
| REN-D19-04 | 19 | `crates/renderer/shaders/include/material_sampling.glsl` (`perturbNormal` Path 1) | The Path-1 gate proves the incoming tangent is non-zero, not that it is non-parallel to `N`; when `T ∥ N` the projection is the zero vector and `normalize()` on it is undefined (0/0 → NaN), propagating through `mat3(T,B,N)` into the shaded normal, the `octEncode(N)` G-buffer write, and every RT ray origin built from it. All three sibling TBN builders in the same tree guard the post-projection length (`parallaxDisplaceUV`, `getRayHitTangentFrame`, and Path 2 by construction), so this is a local omission, not house style. Known producer is REN-D19-03; the guard is what keeps a future importer regression from becoming NaN pixels. | NEW |
| REN-D19-05 | 19 | `crates/nif/src/import/mesh/bs_tri_shape.rs` (the 4th tangent branch) | Guards on `!normals.is_empty()`, which is **vacuous** — `normals` is unconditionally populated earlier (`sse_normals`, else mapped `shape.normals`, else `vec![[0,1,0]; positions.len()]`), so the condition is equivalent to `!positions.is_empty()`, already tested. With no authored normals the branch hands the fabricated placeholder to `synthesize_tangents_yup`, producing a tangent basis derived from data that was never authored — exactly the defect #2363 fixed in `bs_geometry.rs` (guard changed to a separate `normals_authored` flag, pinned by `placeholder_normals_with_uvs_do_not_trigger_tangent_synthesis`); this sibling was not updated. `sse_recon.rs` has the same shape, so an SSE buffer with neither `VF_NORMALS` nor `VF_UVS` reaches it with *both* inputs fabricated. | NEW |
| REN-D19-06 | 19 | `crates/nif/src/import/mesh/tangent.rs` (`extract_tangents_from_extra_data`) | The site of the load-bearing #786 `CalcTangentSpace` swap — Bethesda's `tangents` field holds `∂P/∂V` and `bitangents` holds `∂P/∂U`, so the decoder reads the **second** 12-byte half into `Vertex.tangent.xyz` — has **no test coverage**, while every other tangent producer is unit-tested. Untested consequences: the half-swap itself, the `blob.len() != num_verts * 24` size gate (whose failure is a silent warn + fall-through to synthesis), the exact extra-data name match, and the `zup_to_yup_pos` application to both halves. Code reads correct today; the symptom of a regression is "chrome-looking walls", which this project has a standing rule to *mis*attribute to missing textures. | NEW |
| REN-D20-02 | 20 | `byroredux/src/systems/debug.rs`, `byroredux/src/commands/assets.rs`, `byroredux/src/main.rs`, `byroredux/src/commands/world_info.rs` | #2278's fourteen `GpuTimerSnapshot::*_active` flags exist so `0.0 ms` can be told apart from "this bracket did not run", and #2513 taught the egui panel to render `n/a` — but the other four readers of the same fields still print the raw `f32`: `gpu_breakdown` (the SLOW-FRAME / 1 Hz log line, i.e. the **primary hitch-triage surface**), `skin.coverage` (under a comment still stating the ambiguity as unavoidable), the `bench:` summary line (consumed by `scripts/fsr_bench_report.py`, so a skipped bracket lands in the TSV as a hard `0.000`), and `ctx.upscaler` (whose `UpscalerTelemetry::gpu_ms` has no `_active` mirror at all). Re-opens exactly the ambiguity the plumbing was built to close. Note any bench-line format change must move in lockstep with the report script. | NEW |
| REN-D21-01 | 21 | `byroredux/src/commands/scene.rs` (`mat.set` field arms), `byroredux/src/cornell.rs` | `MAT_FLAG_TRANSLUCENCY` is flag-reachable but **scalar-unreachable**: `mat.set … material_flags 64` sets the bit, but there are no `mat.set` arms for `translucency_subsurface_color` / `_transmissive_scale` / `_turbulence` and no Cornell probe authors them, so they sit at `Material::default()` (`[0;3]` / `0.0` / `0.0`) — and the shader branch terminates in `* mat.translucencyTransmissiveScale`, making the whole term zero regardless of the flag. A regression isolated to the #1147 Phase-2b SSS lobe bisects **clean** against Cornell and only reproduces on FO4+ BGSM content. Same false-all-clear gap #2477/#2514 closed for the Disney lobe and #2249 closed for `ior`. | NEW |
| REN-D23-03 | 23 | `crates/renderer/src/vulkan/context/post_passes.rs` (`record_bloom_pass` + `record_post_passes` order) vs. `docs/engine/fsr3-upscaler-integration-plan.md` status header | The plan's target frame graph says bloom and presentation post-processing consume the upscaled image at output resolution, and its status header names exactly **three** carried items. Bloom is a fourth: it still runs before composite/upscale and samples the raw pre-TAA render-extent HDR, so it enters FSR as part of scene colour and is temporally reconstructed with everything else. No runtime hazard (the pyramid is mip-relative, so the halo's output-relative radius is preserved); the cost is doc rot in the authoritative subsystem document. | NEW |
| REN-D23-04 | 23 | `crates/fsr3-sys/src/lib.rs` (`force_dispatch_failure` + its call site) | `BYRO_FSR_FORCE_DISPATCH_FAIL` is documented "Debug-only" but has **no `cfg` gate** (unlike `debug_checking: cfg!(debug_assertions)` next door), and keys on `var_os(..).is_some()`, so `=0` and an empty value both mean "on". Cached in a `OnceLock`, so it cannot be unset for the process — an environment that happens to carry it latches FSR off for the whole session and degrades to the native blit at reduced render extent. Being live in release is arguably *desirable* (smoke and bench run `--release`, which is where the recovery path needs exercising), so the honest defect is the doc plus the predicate. Also undocumented in `docs/engine/fsr3-troubleshooting.md`, whose failure table is where an operator would look. | NEW |
| REN-D23-05 | 23 | `crates/fsr3-sys/native/byro_fsr3.cpp` (`byro_fsr3_context_destroy`); `crates/fsr3-sys/src/lib.rs` (`impl Drop for Context`) | The wrapper is `delete`d and the out-pointer nulled **only inside `if (result == FFX_API_RETURN_OK)`**. On a non-OK return the wrapper, the `ffxContext`, and everything the provider allocated behind it (pipelines, descriptor pool, its own `VkDeviceMemory` — the tens of MB reported as "SDK working memory") stay live with no remaining owner; `Drop for Context` receives the code, `eprintln!`s it, and drops the `NonNull` with no retry. Because `FrameUpscaler::recreate` destroys and rebuilds on **every resize and preset switch**, a persistently-failing destroy compounds per switch. The one place in an otherwise carefully-ordered teardown chain where memory outside gpu-allocator's view can be stranded past `vkDestroyDevice`. Failure-path only; no failure observed. | NEW |
| REN-D23-06 | 23 | `crates/renderer/src/vulkan/upscaling.rs` (`FrameExtentSet::for_output`) | The `.min(max_image_dimension_2d)` clamp on the SDK-queried **render** extent is dead (the function already rejects an over-limit `output` before the query, and every preset returns render ≤ output). Worse if it ever became live: it would rewrite the render extent *after* the SDK produced it, so `FsrTemporalState::new`'s `jitter_phase_count` and the `render_size` handed to `dispatch` would describe a ratio the SDK never sanctioned — precisely the hand-computed-vs-queried mismatch the module's own doc header exists to prevent. `every_fsr_preset_uses_the_sdk_resolution_query` asserts the unclamped values only. A trap for whoever next raises the output ceiling or adds a preset. | NEW |
| REN-D23-07 | 23 | `crates/renderer/src/vulkan/frame_upscaler.rs` (the `frame parameters absent` arm of `record`) | The `log::error!` string contains ~18 literal spaces mid-sentence — a multi-line literal that lost its `\` continuation. This is the **only** signal for a degradation that latches FSR off for the rest of the swapchain generation, and `docs/engine/fsr3-troubleshooting.md` tells operators to grep for exactly these phrases, so a grep on the wrapped phrase misses it. The sibling message directly below uses the correct form. | NEW |
| REN-D23-08 | 23 | `crates/renderer/src/vulkan/context/mod.rs` (the `ExposureResource::new` fallback), consumed by `record_upscale_pass` / `record_presentation_pass` in `crates/renderer/src/vulkan/context/post_passes.rs` | On the happy path FSR and the tone mapper agree exactly (one `ExposureResource` feeds both, and the SDK's convention matches the shader's — both *multiply* scene colour). On the `ExposureResource::new` failure branch they fall back **independently**: presentation uses `DEFAULT_EXPOSURE` (0.85) while FSR receives a null resource and the SDK substitutes its internal default, whose accessor rewrites a zero texel to `1.0`. Reconstruction would then normalise against 1.0 while the tone mapper grades against 0.85 — a ~1.18× mismatch in the luma domain FSR uses for locking and history rectification. Only reachable if a 1×1 image allocation fails at startup. Worth naming because the fallback constant lives in two places with two different values and nothing ties them together. | NEW |

---

## 6. Prioritized Fix Order

Ordered **correctness → safety → optimization**, with sequencing constraints
called out where fixing one item changes another's reachability.

### Tier 0 — process, blocks everything else (do first, it is not a code change)

1. **Cluster C.** Merge `fix/2460-2461-2462-2463-as-rt-correctness` (or cherry-pick
   `f3babea3`), then re-verify all four closures. Until this lands, #2460's AS
   build-scratch overrun (REN-D9-2026-08-12-01) is live with its tracker entry
   shut, and any future audit will re-derive it from scratch. Add the
   **"fix is reachable from `main`"** gate to the issue-closure workflow.
2. **Fix the audit instructions (§8a).** Six confirmed stale checklist items are
   actively producing false positives and false all-clears. This costs an hour
   and improves every subsequent audit.

### Tier 1 — correctness, live defects with real content impact

3. **REN-D6-2026-08-12-01** — hoist the slot-7 MSN specular read out of the `_ =>`
   arm. 394/394 authored Skyrim `_S.dds` masks are being decoded and discarded
   with a live consumer waiting. Smallest fix with the largest measured content
   impact in the sweep; the third of a family whose other two members were fixed
   today.
4. **Cluster A** — reject bit-31-set samples instead of masking, at all three
   sites (`taa.comp`, `svgf_temporal.comp`, `svgf_atrous.comp`). One coherent
   change; pinnable by source-order guards; no capture required. The à-trous site
   is the unbounded one.
5. **REN-D19-01** — LAND terrain `bitangent_sign`. Every exterior TX01 splat
   normal map on four games is V-inverted. Change `Vertex::new_terrain`'s `w` **or**
   the `terrain.rs` V flip, never both, and land the derivation guard with it.
6. **REN-D10-01 (Cluster D-1)** — reorder `cluster_cull.comp`'s ray-direction
   arithmetic into relative space. Pure shader arithmetic; exterior local lighting
   is dropping out in per-tile patches past ~131 k, and the LOD work just made that
   regime normal.
7. **REN-D19-03 + REN-D19-04** — port #2632's orthogonalize-and-normalize into the
   Z-up degenerate branch, and add the zero-length guard to `perturbNormal` Path 1.
   Fix together: (03) removes the known NaN producer, (04) contains the next one.
   Largest affected corpus in the project (Oblivion / FO3 / FNV interiors).
8. **Cluster B**, in this order: (a) decide and land the mesh-ID write-mask
   contract (REN-D11-2026-08-12-01), (b) then resolve the CPU-gate asymmetry
   (REN-D14-NEW-01), (c) then REN-D14-NEW-02, because (a) and (b) *increase* the
   pixel count reaching that loop. Add the Cornell caustic probe (Cluster B /
   REN-D21-01) as part of (b) so the harness can bisect the result.
9. **REN-D18-01** — one-line `weather_system(world, 0.0)` resample after
   `apply_worldspace_weather` on the transition path. Removes a full-brightness
   noon frame at any game hour.

### Tier 2 — safety and spec conformance

10. **REN-D5-01** — null every handle after destroying it in `caustic.rs` /
    `taa.rs` / `svgf.rs`. Mechanical, source-checkable, zero barrier risk, closes
    a double-free at teardown.
11. **REN-D9-2026-08-12-02** — add `MeshRegistry::geometry_generation` to the
    skin-compute descriptor cache key. CPU-side bookkeeping with a deterministic
    fix; the failure mode is the same device loss the RT side already re-points
    bindings every frame to avoid.
12. **REN-D4-01** — null-before-fallible-recreate in the `in_flight` fence loop,
    and move the `framebuffers` assignment after `recreate_for_swapchain`.
13. **REN-D12-2026-08-12-01** — clamp the indirect draw loop (or add the count
    limb to `should_use_indirect_draws`). Pure-Rust, unit-testable, closes a
    device-lost-class spec violation at an already-declared overflow ceiling.
14. **REN-D5-03** — release GPU handles before both LOD `entities.is_empty()`
    early returns, mirroring `terrain_lod_btr.rs`.
15. **REN-D4-04 / REN-D4-05 / REN-D5-05** — the readback trio. **Documentation
    half now** (the fence-is-sufficient claim contradicts the spec and will be
    copied); **code half only after** the validation-layer run in §7, and the two
    code changes must move together.
16. **REN-D2-02** — promote `MAX_LIGHTS` and add
    `const _: () = assert!(MAX_LIGHTS < 0x3FF)`. Cheap, and it is the guard that
    would catch the CRITICAL version of this.

### Tier 3 — correctness of derived data and contracts (no user-visible defect today)

17. **REN-D3-2026-08-12-01** — add `gpu_instance_glsl_copies_stay_in_lockstep`,
    modelled on the `GpuLight` guard that already lives in the same file. The
    highest-fan-out struct currently has the weakest guard.
18. **REN-D3-2026-08-12-02** and **REN-D10-02** — make the two mixed-convention
    contracts explicit (`dof_params.zw` semantics; `getHitTriWorldPositions`'s
    frame). Both are one-comment-plus-one-decision changes that prevent a
    #1488-class shipped bug.
19. **REN-D23-01** — `view_space_to_meters_factor`. **Blocked on #23-02**: there
    is currently no working FSR bench to measure the change against, and the fix's
    validation *is* a shift in the committed thresholds.
20. **REN-D23-02** — repair `fsr_bench_report.py` (`row.get(key, "-")`) and
    re-take the phase-7 baseline on the stepped-camera harness.
21. **The three VRAM ledger rows** — REN-D5-02/REN-D16-02, REN-D16-03, and the
    open #2679. Recompute from the live constants, add the #1814-style
    self-reporting `log::info!` at both allocation sites, and add the unit test
    tying the documented 1080p figure to `froxel_extent(...)`.
22. **REN-D19-02** — make the MSN Z-reconstruction conditional, preferably at
    load time from the DDS format. **Must land before** the FO4 terrain `_msn`
    binding, at which point it escalates to HIGH across the whole Commonwealth.

### Tier 4 — quality, cost and hygiene

23. **REN-D16-01** — clear the exterior HDR attachment to black (one line, kills
    the colour injection) or move bloom downstream of composite (what #2233's
    rationale actually requires).
24. **REN-D11-2026-08-12-02** — hoist the FSR mask tail policy before the four
    production early returns. Affects the default render path.
25. **REN-D13-02**, **REN-D15-01**, **REN-D15-02**, **REN-D16-04** — the four
    visual-quality mechanisms. All want a look or a capture first (§7); REN-D15-01
    additionally has a real per-frame cost component.
26. **REN-D17-05** — luminance-normalise the sheen tint. Latent today, but it is
    on the reference-validation path, so fix it *before* a sheen producer lands.
27. **REN-D20-01** — drain-and-discard on the hidden-overlay branch. Unbounded
    host-RAM growth in the default configuration.
28. **REN-D2-01 (Cluster D-2)** — one-token clamp symmetry fix in the reservoir
    depth comparison.
29. **The remaining LOW doc/comment rot** — batchable in one or two sweeps. The
    highest-value subset, because each sits on a load-bearing contract: REN-D1-01
    (the #516 invariant), REN-D5-06 (#1782's deferred-scratch route), REN-D4-06
    (`copy_depth_to_history` missing from the doc two open issues point at),
    REN-D9-2026-08-12-05 (the skinned-normal gap), REN-D11-2026-08-12-04 (the
    mesh-ID declaration), and REN-D7-2026-08-12-02 (slot-0 semantics).
30. **REN-D12-2026-08-12-02**, **REN-D1-02**, **REN-D14-NEW-04** — the pure
    optimization tail (redundant fragmentation, a duplicated LRU pass, two
    redundant image views). Last, by definition.

---

## 7. Needs-RenderDoc / needs-validation-layer

**No GPU capture and no validation-layer run happened this sweep.** Every row
below is an **observation with a named confirming signal**, not a
recommendation. **No barrier, render-pass, pipeline-dependency or layout edit is
proposed for any of them.** This is the standing no-speculative-Vulkan-fixes rule
applied uniformly.

### Needs a validation-layer run (`BYRO_VALIDATION=1`, sync validation enabled)

| ID | Observation | Confirming signal |
|---|---|---|
| **REN-D4-04** | Device→host readback performs no availability operation and no `vkInvalidateMappedMemoryRanges`; the fence-is-sufficient claim contradicts the spec's fence section | A host-read hazard reported against the image-health buffer (and the screenshot staging buffer) at the post-fence host access. Also worth logging the resolved `memory_properties()` of `image_health_buffers[0]` on the dev card to see whether `HOST_COHERENT` is actually being granted |
| **REN-D5-01** | The double-destroy only occurs after a real allocation failure inside a real swapchain resize | An induced-OOM run under validation layers. **Note the *fix* is source-checkable and carries no barrier risk** — only confirming the current failure needs the device |
| **REN-D23-H1** | The FSR dispatch-failure recovery barriers have never been exercised under the validation layers. The 900-frame clean run covers the *happy* path (`quality` / `performance` / `native-aa`); `balanced` — the off-by-one rounding case — was not among them. The recovery path is the only place asserting a `GENERAL` old-layout for the output image and a `SHADER_READ_ONLY_OPTIMAL` old-layout for depth, and neither is confirmed on a real submit | `BYRO_VALIDATION=1 BYRO_FSR_FORCE_DISPATCH_FAIL=1` logging **no** `VUID-VkImageMemoryBarrier-oldLayout-01197` naming the upscale-output or depth image, and **no** `SYNC-HAZARD-WRITE_AFTER_WRITE` / `_AFTER_READ` on the upscale-output image, on the forced-failure frame or any after it. **New caveat found this run:** the fault injection *latches* — `FrameUpscaler::record` sets `dispatch_failure` on the first rejected dispatch, so `=1` exercises the recovery path on **exactly one frame per process**, whatever `--bench-frames` says. Any run must grep the log for the dispatch-failure message to confirm the recovery frame actually happened |
| **#2465** (open, carried) | Swapchain `UNDEFINED → COLOR_ATTACHMENT_OPTIMAL` not provably ordered after the acquire | Sync-validation acquire-ordering hazard against the swapchain image at the presentation render-pass begin |
| **#2484** (open, carried, **re-verified present**) | `copy_depth_to_history`'s pre-copy `depth_to_src` omits `DEPTH_STENCIL_ATTACHMENT_WRITE` from `src_access_mask` | Sync-validation WAW/RAW hazard naming `depth_image` between `LATE_FRAGMENT_TESTS` and `TRANSFER` |
| **#2485** (open, carried, **re-verified present**) | `record_upscale_pass` consumes the shared `depth_image`, a consumer the `#870` const-assert comment does not enumerate | Doc half needs no capture; a frame showing the FSR dispatch's depth SRV bound to the single `depth_image` confirms the consumer set |
| **OBS-D11-2026-08-12-A** | Depth is a **single image shared by both frames-in-flight** while all eight colour attachments are per-FIF (`create_main_framebuffers` takes one `depth_view`, not a per-FIF slice). Frame *N+1*'s depth clear/write is ordered against frame *N*'s post-pass depth reads only by whatever fence/barrier discipline `draw_frame` applies — not by the per-FIF resource separation that protects every colour attachment | A sync-val capture across a frame boundary. Whether this is actually under-synchronised is invisible to `cargo test` and to the layers in the common case. **Independently raised by two passes; stopping at the observation** |
| **#2403 follow-up** | The `COMPUTE → AS_BUILD \| FRAGMENT` widening shipped **without** the repro the issue asked for, by the author's own note in `3f5c7a22` | The repro named in the commit body (cluster-cull forced to `Err`, skinned actor beside glass). The widening is additive so it cannot regress — this is outstanding *verification* debt, not a suspected defect |

### Needs a frame capture or a look (visual magnitude, not correctness)

| ID | Observation | Confirming signal |
|---|---|---|
| **REN-D13-02** | TAA edge crawl on geometry-against-sky silhouettes with a parked camera. Mechanism traced end-to-end and grep-confirmed; **perceptual magnitude unmeasured** | An exterior cell in `--upscaler taa` with a parked camera, or a two-frame capture diffing the TAA output along a horizon silhouette |
| **REN-D16-01** | The ≈(0.29, 0.44, 0.70) sky lift is derived analytically from `up[0] = 5V` and `BLOOM_INTENSITY = 0.15`. The **structural** half (bloom's source contains no sky and does contain the clear colour) is established from code alone and needs no capture | A capture of `up_mips[0]` on an exterior frame showing the constant-blue plateau directly |
| **REN-D16-04** | Trailing-edge emissive smear. The asymmetric time constant is proven from shader arithmetic; visibility depends on the local:global `sigma_t` ratio in a given cell | Capture the injection volume over ~15 frames as a flame moves, before any fix lands |
| **REN-D15-01** | Water shades and deposits caustics for depth-occluded fragments. The `early_fragment_tests` fix is semantically free on the blend side, but **verify before landing** | Confirm the water blend is unchanged and measure the occluded-fragment saving on an exterior water cell |
| **REN-D15-02** | Water-side caustic speckle prediction (single-pixel deposits, no footprint, no decay) | An exterior sunlit water cell watching the sub-surface floor for per-frame speckle, or a capture of `water_caustic_accum` showing isolated non-zero texels surrounded by zeros |
| **REN-D2-01** | ReSTIR spatial reuse inert past ~66 841 BU. The arithmetic is provable; the *visual* consequence is not | An FNV/Skyrim `--grid radius>=2` look, or a capture. #2554's own far-field lighting change carries the same caveat by its author's note |
| **REN-D19-02** | Mirrored model-space normals on three-channel `_msn` | A capture against a real `PiperHead_msn` surface. Not observable from `cargo test` |
| **REN-D17-06** | `specularAaRoughness`'s uncited `0.25` and missing threshold cap. **Do not tune blind** | A capture on a high-normal-variance surface (foliage cutouts, chain-link, fine grating) at distance |
| **REN-D18-01** | Whether the one-frame TOD_DAY seed also poisons TAA/SVGF history beyond that frame | A capture across a mid-session worldspace transition. The CPU-side remedy is independent of this question |
| **REN-D1-05** | Whether `shrink_tlas_scratch_to_fit`'s case-2 live-slot arm is genuinely dead | A one-shot `log::debug!` inside case 2 driven through an exterior→interior transition. **Do not** change the shrink/destroy ordering in `draw.rs` — that placement is the #1782-class safety property |

### Explicitly out of scope for attribution

The pre-existing exterior `CopyBufferToImage` VUID predates FSR and **must not**
be attributed to any finding here. No finding in this report attributes any
device-loss or hang to water (`docs/engine/watal.md` §0 records the one
historical "crash near water" report as two non-water bugs, and it is not
revisited).

---

## 8. Process & documentation findings

**These are NOT code bugs and are deliberately excluded from the CRITICAL /
HIGH / MEDIUM / LOW counts in §1.** They are listed with exact file and section
so they can be fixed, because each one corrupts future audits.

### (a) The audit instructions themselves have drifted — six confirmed instances, plus two path corrections

An auditor following a stale checklist either files a **false positive** against
correct code, or — worse — "confirms" the wrong invariant and signs off. The
project already records this failure mode: ~5 of 30 findings in the 2026-04
sweep had stale premises.

| # | File & section | What it says | What the code does |
|---|---|---|---|
| **P-1** | `.claude/commands/audit-renderer/SKILL.md`, Dimension 1 checklist, transform bullet | "`TRIANGLE_FACING_CULL_DISABLE` on all instances (two-sided meshes)" | Gated on `two_sided` since **#416**: `let instance_flags = if draw_cmd.two_sided { TRIANGLE_FACING_CULL_DISABLE.as_raw() } else { 0 };`. Pre-#416, disabling it on every instance caused self-shadowing on far walls and ~2× ray cost on closed meshes. `docs/engine/renderer.md` states the gated behaviour correctly, so the checklist is the only stale copy — and an auditor reading it literally would file the current, correct code as a finding, or "fix" it back to #416's defect. *(Dim 1, REN-D1-06.)* **Also worth adding**: no unit test pins the `two_sided → instance_flags` mapping. |
| **P-2** | `.claude/commands/audit-renderer/SKILL.md`, Dimension 8 checklist | (i) "Fog applied to direct only, not indirect." (ii) "Caustic accumulator (`R32_UINT`) sampled via `usampler2D`." | (i) `composite.frag` applies the volumetric term as `combined = combined * vol.a + vol.rgb` **after** `combined = direct + indirect * albedo + caustic` — the Frostbite §5.3 form over the whole composite, which is physically correct. What is genuinely true (and what #428 was about) is that fog is applied *in composite, downstream of SVGF*, so it never enters the denoiser history. (ii) The glass/MLP accumulator is a **`usampler2DArray`** of three R32_UINT layers (binding 5); only the water-side accumulator (binding 8) is a plain `usampler2D`. `docs/engine/shader-pipeline.md` is **not** stale on either point — only the skill text. A previous pass of this dimension already tripped over the same two lines. *(Dim 8, REN-D8-NEW-05.)* |
| **P-3** | `.claude/commands/audit-performance/SKILL.md`, Dimension 2, "Two-sided blend split gate (#1804)" | Asserts the predicate "requires `z_write` in addition to `is_blend && two_sided`", and calls a split on non-depth-writing batches "the regression" | The live predicate is `is_blend && b.two_sided && b.order_dependent_glass` — **no `z_write` term**. The code's own doc records that the `z_write` spelling was wrong precisely because FO4 BGEM glass is commonly authored `z_write: false`, and the guard test `splits_when_glass_and_z_write_false` asserts a split **does** happen in exactly the case the stale bullet calls the regression. `/audit-renderer` Dim 12 carries the correct spelling, so **the two skills now disagree about the same guard**, and following the stale one re-introduces #1804's original defect. *(Dim 12, REN-D12-2026-08-12-04.)* |
| **P-4** | `.claude/commands/_audit-common.md`, `Volumetrics(M55)` row of the Project Layout block; and `.claude/commands/audit-renderer/SKILL.md`, Dimension 16 checklist | "160×90×128 froxel grid, inject + integrate compute, **single-ray** TLAS shadow, HG phase" | Wrong on **three independent counts**. (i) The grid is **not fixed** — `froxel_extent(render_extent, config)` derives it per swapchain generation. (ii) **128 slices is not a live value anywhere**; `froxel_z_slices` defaults to 64, and 128 is the pre-Session-62 fixed volume that `memory-budget.md` itself records as historical. At a 1080p render extent the live grid is **240×135×64**. (iii) The shadow trace is **not a single ray** — `volumetrics_inject.comp`'s own header documents up to **10** ray-query traversals per froxel (1 opaque sun ray, +1 glass-masked sun ray indoors, then up to `MAX_FROXEL_LIGHTS` local lights × up to 2 rays each), which is the substance of CLOSED #2509. The SKILL checklist repeats (ii) and (iii) and adds "defaults **12** / 64 … so 160×90×64 at 1080p native" — both stale *and* internally inconsistent, since 160×90 is what divisor 12 yields, not 8. **This is load-bearing**: every Dim-16 agent is told to verify the code against these numbers. *(Dim 16, REN-D16-05.)* |
| **P-5** | `.claude/commands/audit-renderer/SKILL.md`, Dimension 17 checklist ("`radius < 0.0` → `isInteriorFill`", "single-tap") and Dimension 18 checklist ("interior fill at 0.6× ambient with `radius = −1` … gating RT shadow on `!isInteriorFill` (symbol-anchored, #1200)") | Three claims | **`isInteriorFill` does not exist** — `grep -rn 'isInteriorFill' crates/renderer/shaders/ byroredux/src` → **0 hits** at merge. The negative-radius encoding was deliberately replaced by an explicit canonical kind: `compute_directional_upload` returns `LightKind::Ambient` ("Returns an explicit canonical kind rather than smuggling the distinction through a negative radius"), `collect_lights` maps it to `(3.0, VisibilityMask::NONE)`, and the shader gates on `if (lightType > 2.5)`. **"Single-tap" is also stale**: the live soft-shadow path is 1–8 taps (`MAX_SHADOW_RAYS = 8`, `clamp(rayBudget.directShadowSamples, 1u, 8u)`, averaged then EMA-accumulated). *(Dim 17 REN-D17-04 overlap; Dim 18 REN-D18-07.)* **See the reconciliation immediately below for the 0.4× / 0.6× question.** |
| **P-6** | `.claude/commands/audit-renderer/SKILL.md`, Dimension 19 checklist | "`DBG_*` catalog (24 entries)" | `DBG_BITS` holds **30** entries — counted programmatically at merge. The Dimension 3 bullet hedges the same figure ("read `DBG_BITS` rather than trusting any figure quoted here"); the Dimension 19 bullet states 24 flatly with no hedge, so an auditor following it literally reports a non-existent drift. The catalog itself cannot drift (a value/no-redeclare pin compares `DBG_BITS.len()` against the `pub const DBG_*` declaration count) — only the prose can, and has. *(Dim 19, REN-D19-07.)* |
| **P-7** | `.claude/commands/audit-renderer/SKILL.md`, Dimension 20 checklist | `dispatches_skipped` is "incremented in `draw.rs`" | The increment lives in `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (the #1857 draw-path split). It is a **skin-coverage** counter on `SkinCoverageFrame` (`crates/renderer/src/vulkan/skin_compute.rs`), not a `GpuPerFrameTimers` field. *(Dim 20.)* |
| **P-8** | `.claude/commands/audit-renderer/SKILL.md`, Dimension 6 (single-boundary call sites) | Names `byroredux/src/cell_loader/spawn.rs` as the cell-path `translate_material` caller | `spawn.rs` is now the **dispatcher**; the call moved into the sibling directory, to `byroredux/src/cell_loader/spawn/mesh_instance.rs`. The single-boundary invariant itself holds — exactly two production callers (`byroredux/src/scene/nif_loader.rs` and `byroredux/src/cell_loader/spawn/mesh_instance.rs`), with the remaining four `#[cfg(test)]`. *(Dim 6.)* |

#### Reconciliation: the 0.4× vs. 0.6× interior-fill disagreement

Dim 17 reported interior fill as **0.4×, not the 0.6× the skill states**; Dim 18
reported **0.6× as verified intact**. Resolved by reading the code at merge:
**both dimensions are right about their own site, and neither contradicts the
other.** There are two different constants, at two different layers, applied in
series:

```rust
// byroredux/src/render/mod.rs — compute_directional_upload, interior arm (CPU)
const LEGACY_INTERIOR_DIRECTIONAL_SOURCE_SCALE: f32 = 0.6;
directional_fade.filter(|f| f.is_finite())
    .unwrap_or(LEGACY_INTERIOR_DIRECTIONAL_SOURCE_SCALE).max(0.0)
```
```glsl
// crates/renderer/shaders/triangle.frag — the lightType > 2.5 arm (GPU)
const float INTERIOR_FILL_AMBIENT_FACTOR = 0.4;
Lo += lightColor * atten * albedo * INTERIOR_FILL_AMBIENT_FACTOR;
continue;
```

The CPU `0.6` is a **fallback for unauthored `directional_fade`** applied to the
XCLL directional colour; the shader `0.4` is the ambient-fill weight applied to
the resulting light. With `directional_fade` unauthored the effective attenuation
of the authored XCLL directional colour is `0.6 × 0.4 = 0.24`.

**The skill checklist is wrong in a third way neither dimension named**: it
attaches the CPU-side constant (`0.6`) to the shader-side concept ("interior fill
at 0.6× ambient"), conflating two layers. The corrected checklist wording should
read: *"interior XCLL directional is tagged `LightKind::Ambient` with
`VisibilityMask::NONE` and an unauthored-fade source scale of
`LEGACY_INTERIOR_DIRECTIONAL_SOURCE_SCALE = 0.6`
(`byroredux/src/render/mod.rs`); the shader's `lightType > 2.5` arm then applies
`INTERIOR_FILL_AMBIENT_FACTOR = 0.4` and `continue`s before any BRDF, reservoir
or shadow ray (`crates/renderer/shaders/triangle.frag`, mirrored by
`if (lightType > 2.5) return false;` in
`crates/renderer/shaders/include/lighting.glsl`). Italicise* isInteriorFill *as a
removed historical name."*

### (b) Project docs describe water as doing vertex displacement — it does none

`water.vert` computes `worldPos = inst.model * vec4(inPosition, 1.0)` and applies
**no offset of any kind**. The vertex stage does not even see `WaterPush.tune.w`.
Its own header states the design — *"The water mesh is always a flat quad in
mesh-local space (no per-frame BLAS rebuild) … Wave detail is added in the
fragment shader as a perturbation of the shading normal; the BLAS sees a flat
plate."* `crates/core/src/ecs/components/water.rs` repeats it, and
`docs/engine/watal.md` §2 has it right.

Four documents disagree, confirmed by `grep -rn "vertex displacement"` returning
these four and **no source file**:

| File | Site |
|---|---|
| **`CLAUDE.md`** | the shader table row — *"`water.vert/frag` Water plane — vertex displacement + RT reflection/refraction (M38)"* |
| **`ROADMAP.md`** | the M38 row — *"dedicated `WaterPipeline` (vertex displacement + Fresnel)"* |
| **`.claude/commands/_audit-common.md`** | the `Water (M38):` layout row — *"WaterPipeline: vertex displacement + Fresnel"* |
| **`.claude/commands/audit-renderer/SKILL.md`** | Dimension 15 checklist, first bullet — *"vertex displacement bounded, no NaN"* |

**`CLAUDE.md` is loaded into every session, so this one misinforms
continuously.** The SKILL bullet is the most immediately harmful: it sends a
reviewer hunting for a clamp on a code path that does not exist, and invites a
contributor to "restore" a feature that was deliberately never built — it would
force a per-frame BLAS rebuild, which WATAL §6 puts out of scope. *(Dim 15,
REN-D15-07.)*

**Fix**: replace "vertex displacement" with "normal-perturbation waves" in all
four, and re-word the SKILL bullet to check the bound that actually exists — the
tangent-space tilt clamp in `sampleScrollingNormal` (`* 0.12 * ampScale`, kept
under the `foamCrest` threshold) and the `NORMAL_PLANE_EPS` sub-plane projection.

**Also**: `byroredux/src/commands/water.rs` is a live console command module
(`water.dump` / `water.contacts`, tests green) and is **absent** from the
`Commands:` row of `.claude/commands/_audit-common.md`, which enumerates every
other per-domain command module. *(Dim 15.)*

### Validator note

`.claude/commands/_audit-validate.sh` **does not accept a file argument** — its
usage is `_audit-validate.sh [--verbose]`, and it validates backticked path
references in the `audit-*/SKILL.md` and `_audit-*.md` files under `.claude/commands/` against the
live tree, not arbitrary documents. It was run at merge and reports
`Checked 1191 refs across 26 skill files. OK: all path references valid.` — i.e.
the skill files' *paths* resolve; what §8a documents is that their *claims* have
drifted, which this validator by design cannot see. Every backticked path in
**this** report was checked by hand against the live tree, including the
directories `byroredux/src/render/`, `byroredux/src/cell_loader/references/`,
`byroredux/src/cell_loader/spawn/`, `crates/renderer/src/vulkan/acceleration/`
and `crates/renderer/src/vulkan/scene_buffer/`.

**When the §8a edits land, re-run `.claude/commands/_audit-validate.sh`.**

---

## 9. Issue-status updates (triage of existing issues, verified this run)

Not new findings — corrections to the tracker's current state. **These should be
actioned before any new issue is filed from this report**, or the new issues will
duplicate them.

| Issue | Tracker state | Verified state | Action |
|---|---|---|---|
| **#2460, #2461, #2462, #2463** | **CLOSED** (credited to `f3babea3`) | **All four live at HEAD** — `f3babea3` is not an ancestor of `main`. See **Cluster C** | **Merge the branch, or reopen all four.** Highest-priority tracker action in this report |
| **#2464** (`DalcCubeUBO` block size unpinned) | OPEN at dimension-agent time | **FIXED at HEAD by `316e085e`** ("pin DalcCubeUBO's block size, and every other unpinned UBO"), which landed *after* every agent ran. Dims 3 and 18 both reported it still open; that is a timing artefact, not an error | Close-verify |
| **#2415** (`gpu_instance_layout_tests.rs` order-test doc quotes 300 B) | OPEN | **Fixed** by `3496a518` — no `300` remains in that file; both doc sites now read "the struct's pinned size" | Close-verify |
| **#2483** (stale byte sizes in `gpu_types.rs` / `constants.rs`) | OPEN | **All-but-fixed** by `3496a518` — `constants.rs` now reads 348 B / 5.7 MB / 11.4 MB and `gpu_types.rs` names `gpu_instance_is_128_bytes_std430_compatible`. **Residual is one phrase**: `constants.rs` still says "the 4 GB total VRAM budget" where the issue also asked for alignment with the 6 GB RT minimum | Close after the one-line **"4 GB" → "6 GB"** edit |
| **#2712** (`lighting`/`flow`/`wrinkle` deferral "recorded only in a one-off audit report") | OPEN | **Premise partly false.** The deferral *is* recorded in `docs/engine/shader-pipeline.md` — the doc `_audit-common.md` designates authoritative for GPU-struct layouts — and was already there at the audited commit (three table rows marking the lanes deliberately **unsampled**, plus the prose "that is intentional, not drift"). The three lanes are still unsampled at HEAD, so the *observation* stands; only the "undocumented" framing does not | **Re-scope to "the deferral is not in the code"** (Rust field docs / GLSL struct block), and note the per-lane DDS is still resolved and uploaded by `map_secondary_texture_handles` |
| **#2711** (phantom-command doc drift) | OPEN | **Reproduces at HEAD, and is under-scoped by one site.** Both documented defects hold (`materials_unique` documented as `== MaterialTable::len()` while `byroredux/src/main.rs` assigns `unique_user_count()`; three `mat.stats` mentions against a registry that only registers `ctx.scratch`). The issue body does **not** cover a third phantom command in the same doc block: `ScratchTelemetry::materials_overflow` (`crates/core/src/ecs/resources/mod.rs`) is documented as "Surfaced by the `mem` console command" — `byroredux/src/commands/` registers `mem.frag` and `ctx.scratch`, never bare `mem` | **Append the third site** rather than filing separately (same block, same defect class) |
| **#2673 / #2674** (TLAS use-after-free / commit-point) | Fixed **today** by `4659cbe0` | **Verified intact from three dimensions independently** (1, 4, 5). `ensure_tlas_state` allocates into locals and retires the old slot past a non-failing commit point (with the staging-buffer `inspect_err` unwind present); `build_tlas` promotes `last_blas_addresses`, clears `needs_full_rebuild` and stamps `last_blas_map_gen` only after `cmd_build_acceleration_structures`. Both source-position pinned by new tests in `crates/renderer/src/vulkan/acceleration/tests.rs` | **Recast as a verified regression guard, not re-reported.** Watch `build_tlas_commits_bookkeeping_after_recording_the_build` and `ensure_tlas_state_allocates_before_destroying_the_old_slot` on future refactors |
| **#2684** (bare `# Safety` sections on FSR barrier fns) | OPEN | **Dedup correction.** The quarantined 11:48 run filed the four bare-`# Safety` FSR barrier functions as a NEW finding (`REN-D23-05`); they are **Existing: #2684 (OPEN)** — that run's dedup miss. Dim 23 caught and reclassified it this pass. Independently, `crates/fsr3-sys` itself is clean: #2544 holds, every `unsafe fn` there carries a `# Safety` section naming owner and lifetime, and **no FFI lifetime violation was found** | **Do not file.** The gap is renderer-side, tracked by #2684 |
| **#2686** (dead `GLASS_RAY_BUDGET` GLSL constant) | OPEN (`SAFE-D7-01`) | **Still true at HEAD** — `grep -c GLASS_RAY_BUDGET crates/renderer/shaders/triangle.frag` → 1, the explanatory comment; the live gate reads `rayBudget.glassRayLimit`. Dim 2 reached it independently | **Existing — not re-filed** |
| **#2273** (stale "75 scalar fields") | OPEN | **Still valid, at two sites** — the `intern_by_hash` collision-policy doc *and* the `hash_gpu_material_fields` doc, both in `crates/renderer/src/vulkan/material.rs`. Live count is **87** (348/4; 87 `h.write_u32` calls, 87 `pub` fields) | Keep open; **add the second site** |
| **#2433** (`gpu_instance_does_not_re_expand_with_per_material_fields` is a no-op citing a stale size) | OPEN | **Still valid** — the body is `let _ = GpuInstance::default();` and the "112 B" mention survives at two sites in the same file | Keep open |
| **#2688** (lockstep never pins the GLSL scalar *type*) | OPEN | **Still valid, and its premise sharpened**: the SPIR-V offsets extracted this run are type-derived, so a `uint`↔`float` swap that preserved size stays invisible to `cargo test` | Keep open |
| **#2697** (`supplemental_texture_indices` is a third hand-written role walk) | OPEN | **Still true, and it bounds a pin**: `material_hash_matches_gpu_material_field_hash` compares two walks of the *same* array, so a slot→field mis-mapping in `to_gpu_material` hashes identically on both sides and passes | Keep open |
| **#2710** (effect-shader glass-carrier promotion lost its texture-keyword arm) | OPEN | **Still live at HEAD** — `classify_glass_into_material` computes `effect_glass_carrier` from `material_kind == MATERIAL_KIND_EFFECT_SHADER && bgem_glass`, with `keyword_match` feeding only the early return; the one test on that arm still pins only the `InnerHaze` direction | Keep open |
| **#2444** (LAND terrain / terrain LOD / object LOD carry no `Material`) | OPEN | **Still true and now slightly wider**: `byroredux/src/cell_loader/terrain_lod_btr.rs` (landed `d96110eb`, after the issue was filed) attaches a `MaterialTextureHandles` carrying a real `.btr` normal map to `Material`-less LOD entities. **Not** an escalation of #2445 — that site hard-sets `normal_has_alpha: false` — but the issue has grown a second consumer | Keep open; note the new consumer |
| **#2721** (three live "100-byte Vertex" doc sites) | OPEN | Still valid. **REN-D11-2026-08-12-05 is a *fourth*, different site** (`pipeline.rs`'s UI builder) | Keep open; consider folding REN-D11-…-05 in |
| **#2466, #2467, #2468, #2473, #2485, #2487, #2489, #2490, #2504, #2507, #2515, #2518, #2519, #2520, #2532, #2571, #2572, #2607, #2677, #2679, #2683, #2685, #2687, #2695, #2700, #2703, #2707, #2719, #2735, #779, #2443, #2445, #2451, #2464(→closed), #2569** | OPEN | Re-confirmed live at HEAD by their owning dimensions and **deliberately not re-reported**. Listed so a reader can see they were checked, not missed | No action |

**Closed and verified still fixed** (recast as regression guards, not
re-reported): #2140, #2143, #2145, #2146, #2156, #2158, #2178, #2200, #2230
*(regressed — see REN-D5-02/REN-D16-02)*, #2233, #2240, #2313, #2363, #2402,
#2403, #2413, #2475, #2480, #2492, #2494, #2499, #2500, #2502, #2505, #2506,
#2508, #2511, #2512, #2513, #2544, #2632, #2693, #2694, #1936, #1969, #1997,
#2469, #2496, #2449, #2474, #1502, #1487, #1488, #1489, #1490, #1496, #1526,
#1642.

---

## 10. Investigated and disproved

**What NOT to re-investigate.** Eight hypotheses were raised during the sweep and
withdrawn after checking. Recording them is the point — each one is a plausible
reading of the code that a future auditor will independently arrive at.

### Dimension 4 — four sync hypotheses withdrawn

1. **Present-failure `render_finished[img]` signal leak.** `draw_frame`'s
   `queue_present` `Err(e) => anyhow::bail!` arm returns without the
   signal-recovery its siblings perform. **Not a finding**: the image was acquired
   and never presented, so `vkAcquireNextImageKHR` cannot return that index again,
   and no second signal on the same handle is reachable. The
   `ERROR_OUT_OF_DATE_KHR` arm is separately covered — it returns
   `suboptimal = true`, the caller runs `recreate_swapchain`, and
   `recreate_for_swapchain` destroys and recreates every `render_finished`
   semaphore.
2. **Host-zeroing of the health counter vs. the shader's `atomicAdd` (WAW).** The
   CPU `bytes[..8].fill(0)` is ordered to the fragment-stage atomic by `draw.rs`'s
   bulk `HOST/HOST_WRITE → VERTEX|FRAGMENT|COMPUTE|DRAW_INDIRECT` global
   `VkMemoryBarrier`, recorded before the render pass and covering all memory.
   `dst_access_mask` carries `SHADER_READ` but not `SHADER_WRITE`; since
   `atomicAdd` is a read-modify-write the read half is in scope and the execution
   dependency is present. **Not pursued.**
3. **Cross-submit ReSTIR reservoir visibility.** Frame N's fragment-stage
   reservoir writes vs. frame N+1's reads, with no barrier and no semaphore between
   the submits. **Not a finding**: the fence's memory dependency *does* include
   device access — the same spec sentence that excludes host access — and
   `draw_frame`'s both-slots `wait_for_fences` retires every prior submission
   before the next is recorded.
4. **`a0a52bc3`'s barrier dedup changing semantics.** All four converted sites
   (`exposure.rs`, `ssao.rs`, `placeholder.rs`, `volumetrics.rs`) were diffed
   field-by-field against the shared helpers. **Identical** apart from two
   deliberate improvements: `QUEUE_FAMILY_IGNORED` now set explicitly (was a
   zeroed default), and `exposure.rs`'s stale `TOP_OF_PIPE` src stage moved to
   `NONE`. `volumetrics.rs`'s `full_range` confirmed to be
   `color_subresource_single_mip()`, matching what the helper builds. **No finding.**

### Dimension 19 — four tangent-space hypotheses disproved

5. **`NiObjectNET`'s pre-`10.0.1.0` single-ref extra-data head** captures only the
   head of the linked list, never walking `next_extra_data_ref`. **Cannot hide a
   tangent blob**: the branch is gated on `version < V10_0_1_0`, and every NIF
   version that ships a `"Tangent space (binormal & tangent vectors)"` blob
   (Oblivion 10.0.1.0 / 10.1.0.0 / 10.2.0.0 / 20.0.0.x and later) takes the
   counted-array branch.
6. **`NiBinaryExtraData` size-prefix contamination.**
   `crates/nif/src/blocks/extra_data.rs` reads `size` as a `u32` and then
   `read_bytes(size)`, so `binary_data` **excludes** the prefix and the
   `num_verts * 24` gate compares like for like. **No systematic authored-tangent
   rejection.**
7. **`NiTriStripsData` reaching synthesis with an empty triangle list.**
   `ni_tri_shape.rs` calls `data.to_triangles()` *before* the `synthesize_tangents`
   call and passes the de-stripped result, so the strips path is **not** silently
   pushed into the degenerate branch.
8. **A missing Z-up→Y-up swap on the model-space normal sample.** The FO4 `_msn`
   measurements (REN-D19-02) show green is the "up" axis (terrain mean G = 0.900),
   i.e. the maps are already Y-up and consistent with `mat3(inst.model)`. **Adding
   a swap would break them. Do not add one.**

### Also considered and dropped

- **Dim 7 — animated-UV dedup churn.** `AnimatedUvTransform` replaces
  `uv_offset`/`uv_scale` per frame and those four scalars are in the dedup hash —
  structurally the same shape as the particle colour fade #1795 quantized away,
  with no quantization here. Dropped: the population is bounded by the number of
  UV-animated entities in a cell (tens), not particle count (thousands), and
  entities sharing a clip phase still collapse. No hit-rate claim can be made
  without a cell run, and inventing one is forbidden.
- **Dim 7 — release-build FxHash aliasing, stale-material re-draw, and dirty-gate
  staleness across resize.** All three checked and clean; the first is already
  documented in-code and counted in debug (#1414).
- **Dim 16 — froxel temporal runaway, FP16 ceiling, self-slot read, cold-start.**
  Established as **non-findings** while checking REN-D16-04 and recorded there so
  they are not re-derived: `historyWeight` is a `clamp(_, 0, 0.98)` times two
  `exp(-x) ≤ 1` factors and `mix(current, history, w<1)` is a contraction with
  fixed point `current`, so no runaway accumulation; magnitudes are far below the
  RGBA16F 65504 ceiling; the previous-slot index is the other FIF slot, barriered;
  `history_valid` starts false and gates the whole block.
- **Dim 20 — `active_bits` set at record time rather than submit time.** A frame
  that records brackets then bails would report `_active = true, _ms = 0.0` for one
  cycle. Confirmed **non-reachable**: no non-fatal early return exists after the
  first bracket site, and all three post-bracket bails propagate `Err`, which
  `main.rs` answers with `log::error!` + `event_loop.exit()`.
- **Dim 2 — the ReSTIR reuse/finalize blocks are not gated on
  `directShadowRayEnabled`, only the streaming is.** On a frame where that flag
  flips true→false while `reservoirsPrev` still holds live entries, the frame would
  both add every light unshadowed to `Lo` *and* re-add the reservoir estimate —
  double-counting the direct term. **No reachable trigger found**: the runtime
  `DBG_*` bits come from `parse_render_debug_flags_env()` once at context
  construction, so a session that starts with direct shadows disabled never
  populates a reservoir (cold-start is self-consistent); a device without ray query
  is cold from frame 0; and the one 1→0 transition that exists is the
  TLAS-build-failure arm, a single already-degraded, already-warning frame. It is
  also **pre-existing**, not introduced by #2554. Recorded as an observation only.

---

## 11. Coverage and limits

**All 23 dimensions ran at depth `deep`.** Entry points from every dimension's
checklist were visited. Dynamic gates actually executed across the sweep:

- **Shader artifact integrity**: all 21 first-party shaders recompiled with the
  CLAUDE.md-documented `glslangValidator -V` and byte-compared against their
  committed `.spv` — **21/21 identical** (independently confirmed by Dims 2, 3, 9,
  13, 15). `cargo build -p byroredux-renderer` regenerated
  `crates/renderer/shaders/include/shader_constants.glsl` with **zero** drift.
- **Tests**: `cargo test -p byroredux-renderer --lib` → **580 passed, 0 failed**;
  targeted runs for `material` (45), `taa` (8), `needs_two_sided_blend_split_tests`
  (7), and the water surface (24 renderer-crate + 30 binary-crate) all green.
- **Binary-level layout validation**: `spirv-dis` on the **committed** `.spv`,
  extracting `OpMemberDecorate … Offset` and `OpDecorate … ArrayStride` — i.e.
  what the GPU reads, not what the GLSL text says.
- **Real game data decoded**: Skyrim SE `Meshes0.bsa` + `Meshes1.bsa` shader-type
  5/6 texture-slot occupancy cross-tabulated against the MSN flag (REN-D6-…-01),
  one `_msn` DDS header extracted from `Skyrim - Textures*.bsa`; FO4
  `Fallout4 - Textures*.ba2` DX10 records decoded for `_msn` / `_n` format
  distribution and per-channel statistics (REN-D19-02). Numbers in those findings
  are **counted, not estimated**.
- **Vendored SDK cross-check**: `third_party/fidelityfx-sdk-v1.1.4` read for the
  four contracts the engine asserts about it (view-space-to-metres consumption,
  the exposure convention and null fallback, the create-flag set, and the
  dispatch-error control flow).

**Not done, by design or by constraint:**

- **No engine launch and no GPU device driven** — the standing
  no-parallel-engine-launch rule. Every visual claim is a mechanism with a named
  confirming signal (§7), never an observed image.
- **No RenderDoc capture and no validation-layer run.** All barrier-semantics
  observations are quarantined in §7; **no barrier, render-pass, pipeline or
  descriptor-layout change is proposed anywhere in this report.**
- **No FPS or millisecond figure is asserted anywhere.** REN-D14-NEW-02 and
  REN-D15-01 both describe real cost mechanisms and both explicitly decline to
  quantify them; REN-D23-02 documents *why* no such figure could be taken even if
  wanted (there is currently no working FSR bench).
- **Not scanned**: FO4 / FO76 / Starfield corpora for the shader-type-5 gap (those
  games source materials from BGSM/CDB where `merge_external_material` can populate
  `specular` independently, so REN-D6-…-01 is stated **for Skyrim only**); a WATR
  census across the seven game data sets (so no claim is made about whether vanilla
  records author `wave_amplitude == 0`); the `crates/ui` Ruffle/wgpu allocator (a
  second, independent GPU allocator with no owner audit skill — #2719 / #2715 are
  its filed symptoms).
- **Out of scope by ownership**: FSR SDK-internal working memory (opaque to
  gpu-allocator, a known gap in `memory-budget.md`'s FSR table); the unexercised
  FP32 shader permutation and `native-aa`'s net perf cost, both documented carried
  scope rather than defects.

**One dimension came back clean**: Dimension 22 (light animation canonical
translation) — zero findings, mirrored-pair invariant intact, every consumer
reading the translated value.

**Scratch traceability**: every finding ID in this report resolves to the same ID
in `/tmp/audit/renderer/dim_N.md`. The quarantined earlier sweep is at
`/tmp/audit/renderer/stale_prior_run/` and produced no report and no issues; its
still-live items are re-filed here under this run's IDs with attribution
(REN-D4-01, REN-D4-02, REN-D4-03, REN-D18-02, REN-D18-07, REN-D15-09,
REN-D11-2026-08-12-06, REN-D11-2026-08-12-07, REN-D2-03).


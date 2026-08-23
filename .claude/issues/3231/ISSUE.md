# Issue #3231: Follow-up to #2221: no morph-target vertex deformation path — AnimatedMorphWeights lands in the ECS but nothing consumes it

**State**: OPEN
**Labels**: animation, renderer

## Body

**Severity**: MEDIUM · **Scope**: new GPU feature, not a bug fix

### Background

#2221 ("non-transform animation channels have no production sink components") landed the ECS-side wiring for `AnimatedMorphWeights` — `anim_convert::attach_animation_sinks` attaches the sink at clip-attach time and `apply_float_channels`'s `MorphWeight(idx)` arm updates it every frame. That half is fully wired end-to-end.

What's still missing: **no vertex-deformation path consumes `AnimatedMorphWeights` at all.** `grep -rin "morph" crates/renderer/src/` returns zero hits — there is no GPU skinning/vertex-deformation infrastructure that reads per-target blend weights.

The raw data is already parsed and then discarded: `crates/nif/src/blocks/controller/morph.rs`'s `NiMorphData.morphs[i].vectors` (per-vertex position deltas per morph target) is captured at NIF-import time, but the only reader anywhere in the tree (`crates/nif/src/anim/channel.rs::resolve_morph_target_index`) reads `morphs[i].name` only — to resolve which weight-channel index a name maps to — and never touches `.vectors`.

### Why this needs its own issue

Wiring this for real requires new GPU infrastructure, not a data-plumbing fix:
- A new compute shader (or CPU-side blend path) that applies per-target vertex deltas weighted by `AnimatedMorphWeights`, likely mirroring the existing GPU pre-skinning approach (`skin_vertices.comp`, M29).
- An SSBO upload path for the per-target delta buffers (parsed today, never uploaded).
- Vertex-format / pipeline changes to consume the deformed positions downstream of skinning (FaceGen lip-sync and talking-head animation are the concrete vanilla use case — `NiGeomMorpherController`).

This is genuinely new-feature/milestone-level GPU work — the shader-struct and pipeline surface it touches is exactly the class of change this project's own conventions flag as too risky to do speculatively (no visual verification tool available outside a live `cargo run` + RenderDoc session).

### Suggested scope for a future session

1. Design the delta-buffer upload (per mesh, per morph target — likely keyed similarly to `SkinSlotPool`).
2. A compute pass that blends `base_position + Σ(weight_i × delta_i)` before the existing skin pass (or fused into it).
3. Wire `AnimatedMorphWeights` as the per-frame weight source.
4. FaceGen lip-sync / talking-head regression scene to verify visually.

### Related

- #2221 (parent) — sink attachment + alpha/color/UV/shader-color/shader-float/flipbook consumers all landed there.

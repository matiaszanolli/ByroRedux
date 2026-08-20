# TD3-2026-08-20-02: GpuWaterParams::uv_offset doc says zw are reserved and cell WATR uploads zero - both false

**Issue**: #3204 — https://github.com/matiaszanolli/ByroRedux/issues/3204
**Severity**: LOW
**Labels**: `low,renderer,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md` § TD3-2026-08-20-02 (Dimension 3 — Stale Documentation & Comments).

**Severity**: LOW · **Effort**: trivial · **Age**: `1a428278` (2026-08-20) claimed the lanes; the doc predates it
**Location**: `crates/renderer/src/vulkan/water.rs:136-138` (the doc comment), contradicted by `byroredux/src/render/water.rs:332-340` and by both GLSL mirrors — `crates/renderer/shaders/water.frag:123-125`, `crates/renderer/shaders/water.vert:130-132`

## Defect 1 — `uv_offset.zw` are documented as free; they are live

The Rust struct doc reads:

```rust
/// xy = authored mesh-water UV offset; zw are reserved for future
/// transform terms. Cell WATR surfaces upload zero.
pub uv_offset: [f32; 4],
```

**Both clauses are false at HEAD.** `1a428278` claimed `.z` for the mesh-water flow-map bindless index (bit-cast) and `.w` for its authored tile scale, and updated **both** GLSL mirrors to say so — but not the Rust struct doc, which is the declaration everyone edits first. Cell WATR surfaces upload `u32::MAX` and a neutral scale, **not zero**.

The uploader carries a correct comment fourteen lines above the write, so the repository now states both versions:

```rust
// byroredux/src/render/water.rs:332-340
// z carries the optional mesh-water flow-map bindless index as
// integer bits; w carries its authored tile scale. Cell WATR
// surfaces upload the u32::MAX index and neutral scale.
uv_offset: [ mat.uv_offset[0], mat.uv_offset[1],
             f32::from_bits(mat.flow_map_index), mat.flowmap_scale ],
```

```
$ grep -n "uv_offset" crates/renderer/shaders/water.vert
131:    // xy = authored mesh-water UV offset; z = flow-map index bit-cast;
132:    // w = authored flow-map scale.
```

## Defect 2 — `absorption.w` is live and under-documented at two of three sites

`crates/renderer/src/vulkan/water.rs:121-123` documents `absorption` as *"Starfield per-channel color-absorption ranges in world units; zero triplet is the legacy scalar-fog sentinel"* — **with no mention of `.w` at all**. The uploader writes `precipitation * rain_response.clamp(0.0, 4.0)` there (`byroredux/src/render/water.rs:312-321`), and `water.vert:105` documents it as `w = precipitation`. `water.frag` is silent on `.w` too. Two of the three declaration sites under-document a live lane.

## Impact

*"`zw` are reserved"* is an **active invitation to reuse a live lane.**

`GpuWaterParams` is 352 B against a 64 KiB UBO with **64 bytes** of real headroom (see `AUDIT_SAFETY_2026-08-20`'s `MAX_WATER_DRAWS` finding). So "there are two spare floats right here" is exactly the wrong thing for the next author to believe — the alternative to reusing them is adding a `vec4`, which **overflows the buffer on essentially every device**.

Same failure shape as `VolumetricsParams.render_origin.w` (#1928) and `GpuCamera.render_origin.w` (#2164), both of which `docs/engine/shader-pipeline.md:212` already calls out by name as *"Not a free slot."*

## Suggested Fix

1. Copy the uploader's comment onto the `uv_offset` struct field.
2. Extend `absorption`'s doc with `w = precipitation × rain_response`, and add the same to `water.frag`.
3. Mark both as **"Not a free slot"** in the same words `shader-pipeline.md` uses for the two prior instances.

## Related

- **#3124** (REN-D15-01) establishes that the three declarations' *field names and order* are identical at HEAD. This finding is about the **semantics documented for the lanes**, which that field-name diff cannot see.
- **#1928** (`VolumetricsParams.render_origin.w`), **#2164** (`GpuCamera.render_origin.w`) — the two prior instances of exactly this trap
- The `GpuWaterParams` descriptor-table gap filed alongside — an offset table in the authoritative doc would give all three sites one reference to diff against
- `1a428278` (claimed the lanes)

## Completeness Checks
- [ ] **SIBLING**: All three declaration sites agree on both lanes — `water.rs` (Rust), `water.vert`, `water.frag`
- [ ] **BOTH-LANES**: `absorption.w` documented too, not just `uv_offset.zw`
- [ ] **NOT-A-FREE-SLOT**: Both carry the explicit headroom warning, matching `shader-pipeline.md:212`'s wording

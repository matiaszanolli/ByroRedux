# Issues 2823, 2824, 2826, 2825

All filed from `docs/audits/AUDIT_RENDERER_2026-08-12b.md`. Domain: renderer.

## #2823 — REN-D21-01: MAT_FLAG_TRANSLUCENCY flag-reachable but scalar-unreachable

`mat.set … material_flags 64` sets the translucency bit, but there are no `mat.set`
arms for `translucency_subsurface_color` / `_transmissive_scale` / `_turbulence`,
and no Cornell probe authors them — they sit at `Material::default()` (`[0;3]` /
`0.0` / `0.0`). The shader branch multiplies by `mat.translucencyTransmissiveScale`,
which is always 0, so the whole term is zero regardless of the flag.

Location: `byroredux/src/commands/scene.rs` (`mat.set` field arms), `byroredux/src/cornell.rs`

Severity: low. Label: bug, renderer.

## #2824 — REN-D23-03: record_bloom_pass runs pre-upscale, doc rot in fsr3 plan

Bloom runs before composite/upscale and samples the raw pre-TAA render-extent HDR,
entering FSR as part of scene colour and being temporally reconstructed with
everything else. No runtime hazard — the pyramid is mip-relative so the halo's
output-relative radius is preserved. The plan doc's frame graph and status header
(naming exactly 3 carried items) are stale — bloom is a 4th.

Location: `crates/renderer/src/vulkan/context/post_passes.rs` (`record_bloom_pass`
+ `record_post_passes` order) vs. `docs/engine/fsr3-upscaler-integration-plan.md`
status header.

Severity: low. Label: documentation, renderer.

## #2826 — REN-D19-02: MAT_FLAG_MODEL_SPACE_NORMALS overwrites authored blue channel

The branch applies BC5 two-channel Z-reconstruction
(`mn.z = sqrt(max(0.0, 1.0 - dot(mn.xy, mn.xy)))`) unconditionally to model-space
normal maps, including three-channel FO4 `_msn` BC3/BC1 textures with a populated
blue channel. Measured: terrain `_msn` (BC3) 45% of texels have z<0;
`PiperHead_msn` (BC3) 42% z<0; only FaceCustomization (BC1) is genuinely two-channel
(B≈0 constant). The reconstruction should be conditional — keep authored `mn.z`
when blue carries signal, fall back to reconstruction only when it doesn't (BC1
case decodes to a constant z=-1, trivially separable).

Also settles: terrain `_msn` mean G=0.900 confirms maps are authored Y-up
(matching imported mesh space) — no additional Z-up→Y-up swap needed.

Location: `crates/renderer/shaders/triangle.frag` — the
`MAT_FLAG_MODEL_SPACE_NORMALS` branch.

Suggested fix: distinguish BC5-vs-BC3/BC1 at *load* time (texture registry /
NIFAL parser→Material boundary), set a material flag, never re-derive per-fragment
in the shader.

Severity: medium (escalates to HIGH once terrain `_msn` binding lands — not yet
bound, `btr_normal_path` only resolves `_n.dds` today). Label: bug, renderer.

## #2825 — REN-D23-04: BYRO_FSR_FORCE_DISPATCH_FAIL has no cfg gate

Documented "Debug-only" but has no `cfg` gate (unlike `debug_checking:
cfg!(debug_assertions)` next door), and keys on `var_os(..).is_some()`, so `=0`
and an empty value both mean "on". Cached in a `OnceLock` so it can't be unset for
the process — an environment carrying it latches FSR off for the whole session,
degrading to native blit at reduced render extent. Being live in release is
arguably desirable (smoke/bench run `--release`, exercising the recovery path
there). The actual defect: doc says debug-only but isn't gated, and predicate
treats `=0`/empty as "on". Also undocumented in
`docs/engine/fsr3-troubleshooting.md`.

Location: `crates/fsr3-sys/src/lib.rs` (`force_dispatch_failure` + call site).

Severity: low. Label: bug, renderer.

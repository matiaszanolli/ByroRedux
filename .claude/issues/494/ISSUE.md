# Issue #494

FO4-BGSM-5: fragment shader apply uv_offset/uv_scale + material_alpha discard

---

## Parent
Split from #411. **Depends on #BGSM-3 (GpuInstance slots) — and is only visible once #BGSM-1 + #BGSM-4 land to populate the slots.**

## Scope

Use the new GpuInstance fields from #BGSM-3 inside `crates/renderer/shaders/triangle.frag`:

### UV transform

Every texture sample must apply the instance UV transform before sampling:

```glsl
vec2 uv = fragUV * vec2(instance.uv_scale_u, instance.uv_scale_v)
        + vec2(instance.uv_offset_u, instance.uv_offset_v);
```

Apply to: base albedo, normal, glow, parallax, env mask, detail, gloss, dark — every texture fetch in the fragment shader hot path.

### material_alpha

Multiply into pre-discard alpha:

```glsl
float alpha = albedo.a * instance.material_alpha;
if (alpha < alpha_threshold) discard;
```

Before the alpha-test `discard`, so BGSM-authored alpha modulates the threshold.

## Completeness Checks

- [ ] **TESTS**: visual regression — render a BGSM-referenced mesh with non-identity uv_offset, compare to baseline. Check in the reference image.
- [ ] **SHADER**: recompile SPIR-V
- [ ] **SIBLING**: UI path (`ui.vert` / `ui.frag`) — verify whether the UV transform applies there too (UI elements may author non-identity transforms via Scaleform)
- [ ] **BENCH**: no measurable regression on Prospector Saloon baseline

## Reference

- Shader Struct Sync memory note
- #BGSM-3 (GpuInstance slots) — prerequisite
- #BGSM-1 + #BGSM-4 — populate the slots; this issue just consumes them
- Audit: `docs/audits/AUDIT_FO4_2026-04-17.md` Dim 6 Stage 4

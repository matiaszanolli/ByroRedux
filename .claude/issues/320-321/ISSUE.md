# Issues #320 + #321: Renderer Reflection & Caustics

## #320: Reflection ray decay — distance falloff + roughness jitter
**Domain**: renderer (shader)
**Severity**: MEDIUM (enhancement)

Two changes to triangle.frag:
1. Stronger distance falloff in `traceReflection` (current 0.005 coefficient too weak)
2. Roughness-driven ray jitter — single sample with IGN noise, GGX-style cone

Locations: triangle.frag:208-245 (traceReflection), triangle.frag:498-528 (call site)

## #321: Caustics pass �� refractive surface light projection
**Domain**: renderer (new pipeline)
**Severity**: MEDIUM (enhancement)

Option A recommended: screen-space caustic splat. For refractive pixels, refract light
through surface, cast ray, splat into caustic_buffer. Composite adds to directLight.

New files needed:
- caustic_splat.comp (shader)
- caustic.rs (Vulkan buffer)
- Modifications to composite.frag, draw.rs, triangle.frag

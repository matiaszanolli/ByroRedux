# Issue #320 — Reflection ray decay: distance falloff + roughness jitter

- **Severity**: MEDIUM (visual quality) | **URL**: https://github.com/matiaszanolli/ByroRedux/issues/320

`triangle.frag:208-245` (`traceReflection`) uses linear `1/(1+0.005*d)` falloff (barely attenuates over 5000u) and casts a single mirror ray regardless of roughness. Result: reflections look "cutting" — sharp at all distances, mirror-flat on rough metals. Fix: bump distance falloff to `exp(-0.0015*d)` (or smoothstep), add roughness²-scaled tangent-space ray jitter (single sample, IGN-seeded like GI).

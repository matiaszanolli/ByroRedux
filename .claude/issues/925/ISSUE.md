---
issue: 0
title: REN-D15-NEW-03: Window portal skyColor hardcoded vec3(0.6, 0.75, 1.0) — interior windows ignore TOD/weather palette
labels: bug, renderer, medium, vulkan
---

**Severity**: MEDIUM (interior windows always show midday-blue regardless of weather / time of day)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 15)

## Location

- `crates/renderer/shaders/triangle.frag:1415` — hardcoded `skyColor = vec3(0.6, 0.75, 1.0)`

## Why it's a bug

Window portal sky color is hardcoded at `triangle.frag:1415`. Interior windows ignore the active TOD palette and weather state, always showing the same midday-blue regardless of whether it's night, dawn, dusk, raining, etc.

User-visible: interior cells with windows look wrong at non-midday times. Megaton vault interiors lit through the windows always look like clear-noon-day.

## Fix sketch

Pull `skyColor` from the active TOD palette instead. The palette is already plumbed to the composite pass (used in `compute_sky` for the non-RT miss-fill path); thread it through to triangle.frag's window portal site via the camera UBO or a dedicated sky-tint UBO field.

```glsl
// Before
vec3 skyColor = vec3(0.6, 0.75, 1.0);

// After
vec3 skyColor = camera.skyTint.rgb; // or skyParams.zenithColor
```

## Completeness Checks

- [ ] **SIBLING**: Verify `compute_sky` (composite.frag) already pulls from this palette source.
- [ ] **TESTS**: Visual: load any interior with a window (Megaton vault, Vault 21), verify sky color shifts with TOD.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md

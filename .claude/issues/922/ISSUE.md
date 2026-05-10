---
issue: 0
title: REN-D13-NEW-01: Caustic source CPU gate over-includes hair / foliage / particle quads / decals / FX cards
labels: renderer, medium, vulkan, performance
---

**Severity**: MEDIUM (over-dispatch — wasted ray budget on non-refractive surfaces)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 13)

## Location

- `crates/renderer/src/vulkan/context/draw.rs:888-893` — caustic-source CPU gate (`alpha_blend && metalness < 0.3`)
- `crates/renderer/src/vulkan/scene_buffer.rs:78` — `INSTANCE_FLAG_CAUSTIC_SOURCE`

## Why it's a bug

The caustic-source gate fires for any alpha-blend + low-metalness surface. That over-includes hair, foliage, particle quads, decals, FX cards — none of which are refractive, none of which produce real caustic patterns.

Each affected pixel runs `max_lights` ray queries against the TLAS. On a foliage-heavy exterior cell the wasted ray budget is significant.

## Fix sketch

Tighten the gate to require an actual refractive material:

```rust
// Before
if material.alpha_blend && material.metalness < 0.3 {
    instance.flags |= INSTANCE_FLAG_CAUSTIC_SOURCE;
}

// After
if material.refraction > 0.0 || material.flags.contains(BSShaderFlags1::REFRACTION) {
    instance.flags |= INSTANCE_FLAG_CAUSTIC_SOURCE;
}
```

Or, more conservatively, require both alpha_blend AND a refraction signal (water surface, glass with IOR set).

## Completeness Checks

- [ ] **SIBLING**: Verify the same gate isn't duplicated elsewhere (e.g. material intern path).
- [ ] **TESTS**: Visual regression: water surface still produces caustics; foliage stops producing them.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md

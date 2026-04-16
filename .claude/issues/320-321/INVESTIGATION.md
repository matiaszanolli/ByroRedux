# Investigation: #320 + #321

## #320: Reflection ray improvements

### Current state (triangle.frag)
- `traceReflection()` (line 211-246): distance falloff uses `1.0 / (1.0 + hitDist * 0.005)` — too weak
- Metal reflection call site (line 597-613): single mirror ray `R = reflect(-V, N)`, no roughness jitter
- Glass reflection (line 520-521): also uses single mirror ray, but glass roughness is typically low so less critical

### Fix plan
1. In `traceReflection`: replace linear falloff with exponential `exp(-hitDist * 0.0015)` for smoother decay
2. In metal reflection block (line 597-613): add roughness-driven ray jitter using existing IGN + buildOrthoBasis + concentricDiskSample helpers
3. All helper functions already exist in the shader (line 147-170)
4. `cameraPos.w` carries frame counter for temporal noise seed

### Scope: 1 file (triangle.frag) + SPIR-V recompile

## #321: Caustics pass

### Scope assessment: LARGE
- New compute shader (caustic_splat.comp)
- New Vulkan buffer module (caustic.rs) following gbuffer.rs pattern
- draw.rs: dispatch caustic compute pass
- composite.frag: sample caustic buffer
- triangle.frag: potentially write refractive mask to G-buffer
- VulkanContext: new descriptor sets, pipeline creation, teardown

**Exceeds 5-file threshold — needs user confirmation before proceeding.**

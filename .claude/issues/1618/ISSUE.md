# LC-D7-01: Stale #869 deferred docstrings on NiWireframeProperty / NiShadeProperty (both fully consumed)

**Severity**: LOW · **Dimension**: D7 (Subsystem Coverage / doc rot)
**Location**: `crates/nif/src/import/material/walker.rs:976-991`
**From**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-06-14.md`

## Description
The walker docstrings for `NiWireframeProperty` and `NiShadeProperty` say renderer consumption is "deferred — tracked at #869." Both are now fully wired:
- NiWireframeProperty → `PipelineKey::Opaque { wireframe }` / `Blended { …, wireframe }` LINE pipeline variant (`crates/renderer/src/vulkan/pipeline.rs:57-67`), selected at draw time.
- NiShadeProperty → `INSTANCE_FLAG_FLAT_SHADING` (`crates/renderer/src/shader_constants_data.rs:126`), consumed in the draw path (`crates/renderer/src/vulkan/context/draw.rs:1775`) and shader.

The comments contradict the live code. #869 is CLOSED.

## Evidence
Docstrings at `walker.rs:976-991` say "Renderer consumption is deferred — tracked at #869." vs the live LINE-variant (`pipeline.rs`) + flat-shading-flag consumption (`draw.rs:1775`, `shader_constants_data.rs:126`).

## Impact
Documentation rot only — misleads the next reader into thinking these properties are dropped. No runtime effect.

## Suggested Fix
Refresh both docstrings to state both properties are consumed (LINE pipeline variant + flat-shading instance flag), and drop the "#869 deferred" reference.

## Related
CLOSED #869 (the now-completed consumption work referenced by the stale comment).

## Completeness Checks
- [ ] **SIBLING**: No other "deferred / tracked at #869" docstrings remain in the import/material walker now that consumption is wired

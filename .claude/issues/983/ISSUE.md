# Issue #983

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/983
**Title**: NIF-D5-ORPHAN-A1: Wire NiLight{Color,Intensity,Radius,Dimmer}Controller consumers — lanterns/campfires/plasma weapons emit constant light
**Labels**: bug, animation, nif-parser, import-pipeline, high
**Parent**: #974 (orphan-parse meta) / #869 (original instance)
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: #974 Band A — orphan-parse follow-up
**Severity**: HIGH (visible drop on every light-emitting prop in vanilla cells)
**Domain**: NIF import + Animation + ECS lights

## Description

All four `NiLight*Controller` types are dispatched and parsed cleanly into `NifScene` but never `downcast_ref`'d by the importer:

- `NiLightColorController` — animated color (RGB)
- `NiLightIntensityController` — animated intensity scalar (NiLightFloatController-aliased)
- `NiLightRadiusController` — animated radius (NiLightFloatController-aliased)
- `NiLightDimmerController` — animated dimming multiplier (NiLightFloatController-aliased)

Dispatch arms live in `crates/nif/src/blocks/mod.rs:622-638`. The block parsers populate `NiTimeController`-shaped data (start/stop time, frequency, cycle type, interpolator ref). `LightSource` ECS component at `crates/core/src/ecs/components/light.rs:12` carries the static `color: [f32; 3]` + `radius: f32` but has no animation slot.

## Impact (current behaviour)

Every lantern, campfire, plasma weapon glow, magic flare, and torch with an authored flicker / pulse / dim controller emits **constant** light. Visible on every interior cell with light-anim props (Megaton bar, Prospector saloon, Whiterun marketplace torches, etc.). Pre-fix the cell looks fine but stationary — the authored mood lighting is silently dropped.

## Suggested fix

Two-part wiring:

1. **Component side** — extend `LightSource` with optional animation fields (or add a sibling `AnimatedLightSource` component for the small fraction of lights that actually need it). Mirroring the `AnimatedVisibility` / `AnimatedAlpha` / `AnimatedColor` pattern already in `crates/core/src/ecs/components/` would be the consistent choice. Fields needed:
   - `color_interpolator: Option<NiColorInterpolator>` (or AnimationClip ref)
   - `intensity_curve: Option<FloatCurve>`
   - `radius_curve: Option<FloatCurve>`
   - `dimmer_curve: Option<FloatCurve>`

2. **Importer side** — in the NIF import path that builds the `LightSource` (currently driven by `LIGH` ESM record loading), follow the `NiTimeController` chain on the parent `NiAVObject`. When a controller of one of the four `NiLight*Controller` types is found, wire its `interpolator_ref` → ECS animation system. The animation system already supports float-channel and color-channel sampling per `core/src/animation/`.

3. **System side** — a new `light_animation_system` (or extension of `animation_system` at `byroredux/src/systems/animation.rs`) writes per-frame `color` / `intensity` / `radius` / `dimmer` into `LightSource` from the sampled curves.

## Completeness Checks

- [ ] **SIBLING**: all four controller types wired in the same PR (don't ship color-anim without intensity/radius/dimmer — they tend to be authored together)
- [ ] **TESTS**: fixture-NIF with each of the four controllers attached to a NiLight; assert the corresponding `LightSource` field animates over time
- [ ] **ECS**: the `LightSource` write path must remain compatible with the renderer's RT light-buffer build (verify pipeline pickup)
- [ ] **DOC**: comment block in `extract_light_data` (or wherever the NIF→ECS bridge lives) cites the four controller types so a future audit sees the wiring
- [ ] **PARITY**: M-LIGHT v1 (#stochastic soft shadows) doesn't break when light intensity animates within a shadow cone

## Source quote (audit report)

> Light flicker / dim / pulse controllers parse cleanly but every lantern, campfire, plasma weapon emits constant light.

`docs/audits/AUDIT_NIF_2026-05-12.md` § HIGH → NIF-D5-NEW-01 (orphan-parse meta).

Related: #974 (meta), #869 (NiWireframeProperty + NiShadeProperty orphan — the original instance of this pattern).


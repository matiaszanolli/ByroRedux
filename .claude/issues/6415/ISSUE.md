# EXT-D4-2026-10-26-01: Underwater god rays take the raw sun colour x intensity and never apply the cloud-shadow transmittance

Labels: low,terrain-exterior,shaders,renderer,bug
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: LOW
- **Dimension**: 4 (sky/weather/sun; sun-site contract in skyal.md §2.5)
- **Tier Violated**: n/a (a direct-sun consumer missing the shared transmittance; not a translation-boundary leak)
- **Game Affected**: all exteriors with water (game-invariant)
- **Location**: `crates/renderer/shaders/composite.frag:925-945` (call site), `:569-605` (`underwaterGodRays`); guard gap in `crates/renderer/src/vulkan/sky_cube.rs:1265-1299`
- **Status**: NEW. Static trace only, unconfirmed on screen.
- **Description**: `296d21912` (and `067ce38ef` before it) added an exterior sun consumer in composite. It scales `mix(params.underwater.rgb, params.sun_color.rgb, 0.35) * params.sun_dir.w * 0.12`, where `sun_color` / `sun_dir.w` are the TOD sun-disc colour and intensity (`sky.sun_color`, `sky.sun_intensity`). Those carry the HNAM sunlight dimmer but not the cloud deck. `c231f5b5f` moved cloud dimming out of the sun and into a per-site multiply by `exteriorCloudSunTransmittance` or the camera column, and skyal.md §2.5 says every direct-sun site must apply it ("a new direct-sun site without it ... is a finding"). The god-ray march does not, and composite has no camera-column input. `camera_cloud_sun_transmittance` reaches `volumetric_sun` and `caustic_sun_transmittance` in `post_passes.rs` but not `CompositeParams`. Under a storm deck an underwater camera still gets clear-sky-strength shafts, while the caustics and terrain in the same view are dimmed. `every_direct_sun_site_applies_the_cloud_shadow` scans `lighting.glsl`, grass, water, caustics and `triangle.frag`, but not `composite.frag`, so the guard cannot see it (the same blind spot that hid #6244).
- **Evidence**: `composite.frag:942-944`: `combined += shaftColor * params.sun_dir.w * 0.12 * underwaterGodRays(...)`; `grep -n "cloud\|transmittance"` over that block finds nothing. The gate is `params.depth_params.x > 0.5` (exterior only) `&& params.underwater.w > 0.0 && params.sun_dir.w > 0.0`. The nearby `volumetric_sun` doc comment says the haze and godrays dim with the terrain key through `cloud_transmittance`, but that is the froxel path, not this one.
- **Impact**: Visible only underwater in an exterior under heavy overcast. Over-bright light shafts, up to roughly the 1/transmittance ratio (~12x at full cover, minus the HNAM dimmer already in `sun_color`). Narrow case, small amplitude (0.12 factor), so LOW.
- **Related**: EXT-D4-2026-10-21-01 (same class, fixed by #6244); skyal.md §2.5.
- **Suggested Fix**: Pass `camera_cloud_sun_transmittance` into `CompositeParams` (the reserved `caustic_flags` / `underwater` lanes or a new tail field, with the std140 offset pin) and multiply the god-ray term by it. Extend the guard to scan `composite.frag` for the underwater sun consumer.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader copies, other sun/lighting sites, other rings)
- [ ] **TESTS**: A regression test pins this specific fix

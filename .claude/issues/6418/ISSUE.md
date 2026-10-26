# EXT-D5-2026-10-26-01: Underwater god-ray march hard-codes the water IOR (`1.333`) that #4010 removed from the caustic path

Labels: low,water,shaders,bug
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: LOW
- **Dimension**: Water translation (WATAL) / composite
- **Tier Violated**: no-fabrication (a canonical authored field is re-derived as a literal at the consumer)
- **Game Affected**: all (cosmetic, underwater only)
- **Location**: `crates/renderer/shaders/composite.frag:574` (`underwaterGodRays`)
- **Status**: NEW (same class as closed #4010; different site)
- **Description**: `refract(-sunDirection, vec3(0.0, 1.0, 0.0), 1.0 / 1.333)` fixes the air-to-water index at the clean-water default. #4010 fixed exactly this in `water.frag`'s caustic ray, which now reads the authored `WaterMaterial::ior` (`water.frag:1463-1468`). Its comment says a hard-coded 1.33 made "the caustic focus disagree with the refraction seen through the same surface". The camera's `UnderwaterView.surface` already ships depth and fog span through `caustic_flags.zw`, and `UnderwaterView` has no IOR lane.
- **Evidence**: `composite.frag:574`; no `WATER_IOR` or equivalent in `shader_constants_data.rs` or core. `grep 1.333` over the shaders finds this line only.
- **Impact**: The shaft convergence and focus do not follow an authored or tuned index (Skyrim, FO4 and Starfield author it; FO3/FNV/Oblivion use the default). Visual only, and small, since the default is physically correct for water.
- **Related**: #4010; `watal.md` (god-rays section) already calls the march's scales/sharpness/energy factor "chosen values".
- **Suggested Fix**: Pass the submerged plane's `ior` through the reserved `UnderwaterView.surface` lanes and use it in the `refract()`. Otherwise name `1.333` in `shader_constants_data.rs` as the clean-water default and cite it. The other chosen march values (`g = 0.6`, 24 steps, 70/37 BU focus scales, ridge^6, 0.12) could be named in the same pass for the #6392 discipline.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader copies, other sun/lighting sites, other rings)
- [ ] **TESTS**: A regression test pins this specific fix

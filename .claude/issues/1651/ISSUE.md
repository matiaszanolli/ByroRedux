**Severity**: HIGH · **Dimension**: BGSM/BGEM Consumption (parser → Material boundary; format-translation invariant) · **Status**: NEW (from AUDIT_FO4_2026-06-18)

## Description
`merge_bgsm_into_mesh` forwards the BGSM/BGEM `alpha_blend_mode.src_blend`/`dst_blend` **verbatim** (`as u8`) into `mesh.src_blend_mode`/`dst_blend_mode`. But the BGSM field is documented as a **GL-style enum** (`base.rs:46`: `Zero=0, One=1, SrcColor=2, SrcAlpha=6, InvSrcAlpha=7`), whereas the renderer's `gamebryo_to_vk_blend_factor` (`pipeline.rs:110-111`) reads `mesh.*_blend_mode` as the **Gamebryo NiAlphaProperty** enum (`0=ONE, 1=ZERO`, then 2..=10 follow the D3D ordering). The two enums are **inverted exactly at values 0 and 1**. The in-code comment at `asset_provider.rs:1431-1434` claims they "align 1:1" and lists "0=Zero, 1=One" — which is the GL table, contradicting the renderer's mapping. This is the identical bug class fixed for the *particle* presets in #1649 (`067a8354`), still present on the *material* path.

## Location
- `byroredux/src/asset_provider.rs:1436-1439` (BGSM), `byroredux/src/asset_provider.rs:1529-1532` (BGEM)
- misleading comment `byroredux/src/asset_provider.rs:1431-1434`
- enum source `crates/bgsm/src/base.rs:46-47`
- renderer consumer `crates/renderer/src/vulkan/pipeline.rs:108-123`
- wrong-behavior test `byroredux/src/asset_provider.rs:2398-2413`

## Evidence
- BGSM enum (GL): `crates/bgsm/src/base.rs:46` — `src_blend / dst_blend: GL-style enum (Zero=0, One=1, ...)`.
- Renderer enum (Gamebryo): `crates/renderer/src/vulkan/pipeline.rs:110-111` — `0 => ONE, 1 => ZERO`. Cross-checked against the NIF walker which writes `0 // ONE` into the same `*_blend_mode` fields.
- Verbatim forward, no conversion: `asset_provider.rs:1438-1439`, `:1531-1532`.
- Downstream chain (live on REFR + precombine static-mesh paths): `mesh.dst_blend_mode` → `AlphaBlend{src,dst}` (`spawn.rs:953-954`) → `DrawCommand.dst_blend` (`render/static_meshes.rs:183-184, 550-551`) → `gamebryo_to_vk_blend_factor`.
- **Test pins the wrong behavior**: `asset_provider.rs:2398-2413` ("additive blend (function=2) with One/One factors — common on FO4 effect / glow card BGEMs") asserts `src==1, dst==1` are forwarded unchanged. Under the renderer's enum that is `(ZERO, ZERO)` → the surface contributes nothing to the framebuffer.

## Impact
Cross-game (every game whose materials go through BGSM/BGEM — FO4, FO76, Starfield) and now widened to the FO4 precombine path by `efd3c41b`. The common "Standard" mode `(6,7)` is unaffected (6/7 coincide), which masks the bug for the bulk of glass/decal content — but **additive** BGEM glow/effect/holotape/screen cards (`function=2`, `(One, One)` = `(1,1)`) invert to `(ZERO, ZERO)` and render black/invisible instead of additive, and any BGSM authoring src/dst `0` (Zero) or `1` (One) is corrupted. Bounded to the additive/Zero/One blend population, but the failure is "visible FX surface disappears," not a subtle shift.

## Related
#1649 / `067a8354` (the particle-side twin, fixed by flipping presets to `dst_blend:0`); `efd3c41b` (routed the precombine path through this merge, widening exposure). Project invariant: *Format Translation Layer — translate at the parser→Material boundary, never forward a foreign enum.*

## Suggested Fix
Convert the BGSM/BGEM GL-style src/dst into the Gamebryo NiAlphaProperty enum at the merge boundary (swap only `0↔1`; values 2..=10 coincide) — e.g. a `gl_to_gamebryo_blend(u32) -> u8` helper applied at `:1438-1439` and `:1531-1532`. Fix the misleading comment at `:1431-1434` and update the test at `:2398-2413` to assert the *converted* `(One,One)` → Gamebryo `(0,0)` (which maps to VK `ONE/ONE` = additive). Mirror the #1649 fix's "renderer speaks the Gamebryo enum" discipline.

## Completeness Checks
- [ ] **SIBLING**: Both BGSM (`:1436-1439`) and BGEM (`:1529-1532`) merge branches converted; particle-path #1649 fix kept consistent
- [ ] **CANONICAL-BOUNDARY**: Conversion stays at the parser→`Material` merge boundary; the renderer continues to speak only the Gamebryo enum — no per-game blend logic pushed into shaders/pipeline
- [ ] **TESTS**: The test at `asset_provider.rs:2398-2413` is updated to pin the *converted* additive case `(One,One)` → Gamebryo `(0,0)`, plus a Standard `(6,7)` no-op regression case

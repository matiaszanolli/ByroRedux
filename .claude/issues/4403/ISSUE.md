# #4403 — NIFAL-D8-2026-09-14-04: Engine docs still describe the pre-#3901 flipbook contract (`texture_slot: u32`, renderer bind "deferred")

**Labels**: low,nifal,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (doc)
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: — (doc describes the removed raw-slot leak as the live shape)
- **Game Affected**: all (Oblivion/FO3/FNV flipbook content)
- **Location**: `docs/engine/animation.md:121-125`, `docs/engine/nif-parser.md:904-906`
- **Status**: NEW (same class as OPEN #4360, which does not list these lines — fold in)
- **Description**: The core `TextureFlipChannel` now carries `role: FlipTextureRole` (`crates/core/src/animation/types.rs:218`). `docs/engine/animation.md` still shows `texture_slot: u32 // raw TexType enum`. `docs/engine/nif-parser.md` still calls the renderer bind deferred, which has been false since #2221 (base role) and #3901 (all roles).
- **Evidence**: grep hits at the cited lines; `docs/engine/animation.md` was last touched before #3901.
- **Impact**: A reader following the doc would reintroduce a raw slot on the canonical channel — exactly the leak #3901 closed.
- **Related**: #3901, #4360, #2221.
- **Suggested Fix**: Update both docs to the shipped `FlipTextureRole` contract; fold into #4360.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

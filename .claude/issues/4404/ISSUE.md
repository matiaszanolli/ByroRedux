# #4404 — NIFAL-D5-2026-09-14-02: The #4167 Particles completeness guards have holes — the structural scan matches comments and substrings, and the value test's `src_blend` equals the preset's

**Labels**: low,nifal,test-gap,bug
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (test harness; no runtime impact today)
- **Dimension**: Particles
- **Tier Violated**: single-boundary (the boundary's completeness guard can pass while an overlay is dropped)
- **Game Affected**: all
- **Location**: `byroredux/src/systems/particle.rs:806-847` (structural guard, `body.contains(name)`), `:739` (`Some(6)` passed as `src_blend`), `:773` (assert); `crates/core/src/ecs/components/particle.rs:435` (`torch_flame().src_blend: 6`)
- **Status**: NEW (related #4167, closed by `f680df2e0`)
- **Description**: The structural guard checks raw body text with `contains`, comments included and with no identifier boundary. Most parameter names also appear as substrings of preset fields (`effect_shader` ⊂ `effect_shader_flags`, `greyscale_lut` ⊂ `greyscale_lut_index`), of helper names, or in comments. Separately, the value test passes `src_blend = Some(6)`, identical to the preset, so deleting only the `src_blend` overlay passes both guards.
- **Evidence**: The orchestrator confirmed `byroredux/src/systems/particle.rs:739` passes `Some(6)` inside `every_overlay_parameter_reaches_the_preset` and that `torch_flame()` has `src_blend: 6`. The agent replayed the guard's scan on four mutated copies of the file; the guard passes on all four, including (D) a new parameter mentioned only in a body comment — the exact case the guard exists to catch.
- **Impact**: False confidence; `src_blend` has no working pin.
- **Related**: #4167, #2300, #1513, NIFAL-D7-2026-09-14-04 (same vacuity class).
- **Suggested Fix**: Strip `//` comments before scanning and match whole identifiers not preceded by `.`. Change the fixture's `src_blend` to a value `torch_flame()` doesn't use, and `assert_ne!` every pinned field against `before`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

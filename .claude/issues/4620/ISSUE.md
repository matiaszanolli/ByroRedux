# NIF-D4-2026-09-21-01: sequence_emitter_rate (tier d) is a whole-scene first-match; hundreds of vanilla particle systems get a sibling's authored birth rate

**Issue**: #4620
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: MEDIUM. This is the NIFAL particle-emitter-params row: a translatable parameter silently misattributed, the same class as #4550.
**Dimension**: Geometry Extraction & Import Handoff (particle emitters)
**Game Affected**: FO3 / FNV (`bsver` 34); Skyrim LE / SE (`bsver` 83 / 100); any title with manager-driven emitter blends.
**Location**: `crates/nif/src/import/walk/emitter.rs`:
- `:581-625`: `sequence_emitter_rate(scene, curves)`, which takes no system or controller argument;
- `:697`: the call site inside `resolve`.
- Per-system callers: `import/walk/mod.rs:666` and `emitter.rs:818`.

## Description
When a system's own `NiPSysEmitterCtlr` interpolator is a manager-driven `NiBlendFloatInterpolator` with empty `items` (the #3329 shape), tier (d) scans **every** `NiControllerSequence` in the scene. It returns the first controlled block whose `controller_type` contains "EmitterCtlr" in the best-ranked sequence, without checking which controller that block drives. As a result, every system in a multi-emitter NIF receives the same rate. The file does carry the per-system answer: each `ControlledBlock.controller_ref` points at exactly one emitter controller, whose `target_ref` is the system — `sequence_emitter_rate` just never consults it.

Confirmed at HEAD `ee6d3fb39`: `sequence_emitter_rate` at `emitter.rs:581` still takes only `(scene, curves)`, and the loop at `:646-664` breaks on the first controlled block matching `ctype.contains("EmitterCtlr")` with no comparison against the calling system's own controller.

## Evidence (probe TIERD, mirrors the rejected-curve pass)
- **FNV**: 1,147 systems reach tier (d). **690 systems in 116 NIFs** get a rate different from their own controlled block's rate.
  - `fxravenrockexplosion01.nif`: all six systems get 60/s; their own rates are 30, 150, 30, 4.5, 90 and 30.
  - `dlc05zetadead.nif`: all systems get 18/s; own rates range from 1.5 to 300.
- **FO3**: 712 systems reach tier (d); **386 in 99 NIFs** are misattributed.
  - `explosionlarge01.nif`: 900/s for all, against 60 / 7.5 / 90 / 30 / 60 / 21.4.
  - `vgeardoorr01.nif`: 30/s against 6–180.
- **Skyrim SE**: 1,055 systems reach tier (d); **313 in 150 NIFs** are misattributed.
  - `dlc1vampchangeexplosion`: 180 against 450.
  - `sprigganfxtestunified`: 4.5 against 7.5–99.
- The opt-in gate `real_archive_torch_meshes_surface_particle_emitters` passes, because it counts only *presence* of a positive rate, not per-system correctness.

## Impact
Particle density in multi-emitter explosions, fire, magic, door and vehicle FX is wrong per system, by up to about 120×, on three titles.

## Related
#1402, #4261, #4467, #4550 (all closed — earlier fixes for the same first-match class at other tiers); #4560 (open — NIFAL-D5-2026-09-21-02, the *legacy* `NiPSysEmitterCtlrData` tier below this one; different function/mechanism, not the same bug); #3329 (introduced tier d).

## Suggested Fix
- Pass the system's own emitter-controller block index into `sequence_emitter_rate`, and accept only controlled blocks whose `controller_ref` matches it.
- For controlled blocks without a controller ref, fall back to matching `node_name` against the system's name. Without that fallback, the 47 FNV / 80 FO3 / 193 SSE systems that have no controlled block of their own would drop to the preset.
- Add a multi-emitter fixture asserting distinct per-system rates.

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D4-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other tiers in `emitter.rs`, other first-match resolution paths)
- [ ] **CANONICAL-BOUNDARY**: The fix keeps per-game logic at the NIFAL parser→canonical boundary in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate` family) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix (multi-emitter fixture with distinct per-system rates)

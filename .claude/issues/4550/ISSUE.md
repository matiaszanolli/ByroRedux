# NIFAL-D5-2026-09-21-01: Zero-own-budget particle system inherits a sibling's authored budget via the whole-scene fallback

**Labels**: medium, nifal, nif-parser, bug

**Severity**: MEDIUM · **Dimension**: Particles · **Tier Violated**: no-fabrication · **Game Affected**: Oblivion / FO3 / FNV / Skyrim / FO4 (multi-emitter NIFs; Starfield N/A per #2354)
**Location**: `crates/nif/src/import/walk/emitter.rs:355-380` (`extract_emitter_max_particles`)
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
`extract_emitter_max_particles` only returns from the per-instance `data_ref` path when the own block yields a budget `> 0` (`.filter(|m| *m > 0)` at :360). When the system's own `data_ref` resolves to its own data block and that block authored `0` (parsed to `None`, documented semantic "no authored budget → keep the preset", `blocks/particle.rs:167-177`), execution falls through to the whole-scene `find_map` scan — written for *unresolvable* refs and tested only as `null_data_ref_falls_back_to_the_whole_scene_scan`. In a multi-emitter NIF the scan returns the first budget-bearing block in block order, i.e. a **sibling** system's budget, which `apply_emitter_overlays` then clamps and writes into this system's `preset.max_particles`.

### Evidence
Own-ref hit requires `.and_then(|d| d.max_particles).filter(|m| *m > 0)`; both `None` (authored 0) and a non-`NiPSysBlock` target drop to the scene-wide scan (:379 has the same filter shape on the scan path). The existing zero-budget test uses a single-block scene, so the zero-own-budget + sibling-with-budget shape is untested.

### Impact
The emitter's pool silently jumps from its heuristic preset (~64–96) to `min(sibling_budget, 256)` — unlogged, game-agnostic, exactly on the multi-emitter NIFs #4261 measured at 67.3% of Oblivion+DLC content. Authored-0 budgets are rare, so frequency is low but the wrong value is silent.

### Related
#4261 (per-instance attribution — residual hole in that fix), #3344

### Suggested Fix
When `data_ref.index()` is `Some` **and** the target downcasts to `NiPSysBlock`, return its budget as-is (`None` stays `None`); reserve the whole-scene scan for NULL/non-resolving/non-downcasting refs. Add the two-system fixture (A budget 0, B budget 5000 → A keeps preset, B gets its own).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **CANONICAL-BOUNDARY**: The fix keeps per-game logic at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

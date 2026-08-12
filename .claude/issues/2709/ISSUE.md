# #2709: SF-2026-08-12-D9-03 - `merge_external_material`'s `bool` return cannot distinguish "resolved and populated" from "resolved and forwarded nothing", and all five production call sites discard it anyway

- **Severity**: LOW
- **Dimension**: 9 — external material flow
- **Location**: `byroredux/src/asset_provider/material.rs:667-739`; call sites `byroredux/src/cell_loader/references/import.rs:113`, `byroredux/src/cell_loader/partial.rs:115`, `byroredux/src/scene/nif_loader.rs:273`, `byroredux/src/cell_loader/precombined.rs:275`
- **Status**: NEW
- **Description**: The function documents `touched` as "flips to `true` on any merged
  field", but the `.mat` arm returns `true` after setting only `is_pbr` and forwarding
  no textures, scalars, or alpha state. Every production call site ignores the result
  (only the tests in `byroredux/src/asset_provider/tests.rs` assert on it), so there is no telemetry
  anywhere distinguishing "this cell's materials resolved" from "this cell's materials
  resolved to nothing" — which is precisely the state 97.9% of Starfield content is in.
- **Impact**: Diagnostics only, but it is the reason a total texture blackout (Dim 8)
  produces no log line and no counter. There is no `tex.missing`-style signal on the
  material side.
- **Suggested Fix**: Either mark the fn `#[must_use]` and have callers accumulate a
  per-cell "materials resolved / of which empty" counter, or return a small enum
  (`Unresolved` / `Merged { fields: usize }` / `PresenceOnly`) so the `.mat`
  presence-gate case is nameable.

---

---
**Source**: `docs/audits/AUDIT_STARFIELD_2026-08-12.md` (finding `SF-D9-03`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


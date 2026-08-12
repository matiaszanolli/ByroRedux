# #2707: SF-2026-08-12-D8-01 - `classify_legacy_pbr` stamps a fabricated `Some(0.0)/Some(0.85)` PBR pair onto 97.9% of Starfield meshes from an input set that is empty by construction, permanently disabling the NaN-sentinel fallback

- **Severity**: MEDIUM
- **Dimension**: 8 — NIFAL canonical material translation
- **Location**: `crates/nif/src/import/material/mod.rs:1194-1218` (`classify_legacy_pbr`), `:1269-1270` (the unconditional `Some(...)` write), `crates/core/src/ecs/components/material.rs:816-842` (`resolve_pbr`), `byroredux/src/asset_provider/material.rs:726-739` (the `.mat` early return)
- **Status**: NEW — distinct from #2359, which is about the *merge* forwarding nothing; this is about the *importer* asserting a resolved value it did not derive from anything
- **Description**: On a Starfield material-reference stub the walker returns at
  `dedicated_shader.rs:86` before writing a single `MaterialInfo` field, so
  `into_imported_material` calls `classify_legacy_pbr` on an all-defaults
  `MaterialInfo`: `texture_path = None` → `path = ""` (no keyword can match),
  `specular_authored = false`, `has_normal_map = false`, `has_gloss_map = false`,
  `env_map_scale = 0.0` (the `MaterialInfo::default`, `mod.rs:1061`, which fails the
  `> 0.3` arm). Every classifier arm falls through to the terminal
  `PbrMaterial { roughness: 0.85, metalness: 0.0 }` (`material.rs:757-759`). That
  constant is then written as `metalness_override: Some(0.0)`,
  `roughness_override: Some(0.85)` — indistinguishable downstream from an authored
  value.
- **Evidence**: 38,120 of 38,930 sampled Starfield meshes (97.9%) take this exact
  path. Because both overrides are `Some`, `translate_material` never seeds the NaN
  sentinel, so `Material::resolve_pbr`'s backstop (`material.rs:817`) is unreachable
  for Starfield — the `merge_external_material` comment at `material.rs:730-737`
  states this outcome explicitly ("the NaN-sentinel path in `Material::resolve_pbr`
  never fires for Starfield content") but frames it as a benign fact rather than a
  fabrication.
- **Impact**: (a) Today: a single invented matte-dielectric constant on essentially
  all Starfield content, presented to the Disney BSDF lobe as resolved data.
  (b) After #2359 Phase 2 lands: any `.mat` the CDB index *misses* will silently keep
  the fabricated `0.0/0.85` instead of falling back to the sentinel — the failure
  becomes permanently invisible rather than merely current. This is the NIFAL
  no-fabrication rule (`docs/engine/nifal.md`) applied at the boundary.
  Scored MEDIUM rather than the HIGH the severity table assigns to "divergent
  Material out of NIFAL", because the value is not *divergent* from a competing
  authored value — there is none — and the immediate rendering harm is wholly
  contained by #2359.
- **Related**: #2359, #2353, #2330 (second spawn-time roughness write outside the boundary).
- **Suggested Fix**: When `MaterialInfo` carries no authored signal at all (the
  stub-guard case), leave `metalness_override`/`roughness_override` as `None` so
  `translate_material` seeds the NaN sentinel and `resolve_pbr` owns the default —
  one code path for "unknown", instead of a fabricated `Some` that outranks Phase 2's
  own miss-detection.

---

---
**Source**: `docs/audits/AUDIT_STARFIELD_2026-08-12.md` (finding `SF-D8-01`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


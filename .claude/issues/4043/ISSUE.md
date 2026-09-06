# #4043 — REN-2026-09-06-D6-03: `translate_texture_only_material`'s contract prose is falsified by its own body and by its sibling guard

**Labels**: low, nifal, renderer, shaders, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D6-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs`
  (`translate_texture_only_material`'s doc block, against its own literal and
  against `every_exterior_spawner_inserts_a_boundary_material` in the same
  file)
- **Status**: NEW
- **Description**: This is the boundary's **second** production `Material`
  producer — the one for drawn surfaces with no source material record. Its
  rustdoc makes two load-bearing claims, and both are false against the live
  code.

  1. *"Three exterior draw populations are in this shape: LAND terrain
     (`cell_loader/terrain.rs`), distant terrain LOD (`terrain_lod.rs`), and
     object-LOD imposters (`object_lod.rs`)."* There are **five caller files
     and six call sites**.
  2. *"This is deliberately not a fourth ad hoc materialization site: it owns
     no scalar literals of its own. Every canonical value it produces comes
     from `Material::default()` or from `resolve_pbr`'s classifier."* It owns
     one — and it is the one that matters.
- **Evidence**: `rg -n "translate_texture_only_material" --type rust`,
  production sites only: `cell_loader/terrain.rs`, `cell_loader/object_lod.rs`,
  `cell_loader/terrain_lod.rs`, `cell_loader/terrain_lod_btr.rs` (#3336), and
  `cell_loader/water.rs` **twice** (#3733). The guard test 70 lines below the
  doc already knows this — its own failure message enumerates "the 6 known
  spawners (terrain, terrain_lod, object_lod, placement_lod, terrain_lod_btr,
  water)". Two statements about the same set, one file apart, disagreeing.

  For claim 2: the literal is
  `Material { texture_path, metalness: f32::NAN, roughness: f32::NAN,
  env_map_scale: 0.0, ..Material::default() }`. `env_map_scale: 0.0` is a
  scalar literal that **deliberately deviates** from `Material::default()`'s
  `1.0`, and it carries a 20-line comment explaining that `Material::default()`'s
  value "is the raw on-disk `BSShaderPPLighting` field value" and that
  inheriting it "would have switched distant terrain and LOD imposters into
  full-strength environment reflections as a side effect of a PBR-scalar fix".
  It is pinned by an assertion (`assert_eq!(m.env_map_scale, 0.0)` over three
  fixtures). The deviation is correct; the sentence saying it does not exist
  is not.
- **Impact**: Claim 2 is the *entire* argument for why this second producer is
  not a NIFAL boundary violation, so an auditor who checks it finds it false
  and has to re-derive the real argument ("it owns one documented deviation")
  from scratch. It also matters forward: the `..Material::default()` tail
  means a **newly added canonical `Material` field silently reaches all six
  exterior draw populations at its `Default` value**, with no compile error —
  the opposite of `translate_material`, whose exhaustive literal makes that
  impossible. `Material::default()` is demonstrably *not* a neutral-value
  struct (the `env_map_scale` comment says so outright), so the class of bug
  this permits has already happened once and was caught by a human, not a
  gate. Claim 1 additionally means mesh/ESM water — a population with
  different optical expectations from terrain — is absent from the
  documentation of the function that materializes it.
- **Related**: #2444 (the finding that created this function), #3336, #3733
  (the two spawners that arrived after the prose was written), #3073 (the
  named-default doctrine), #3912 (the same doctrine applied yesterday).
- **Suggested Fix**: Rewrite both sentences: name the five caller files (or
  better, point at `every_exterior_spawner_inserts_a_boundary_material`, which
  already owns the set), and state the real invariant — "one deviation from
  `Material::default()`, `env_map_scale = 0.0`, documented below". Consider
  pinning the deviation count: a source-scan of this function's literal
  asserting exactly one non-`Default` scalar assignment would make a second
  one a deliberate act rather than an accident.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

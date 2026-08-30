# #3568 — REN-2026-08-30-D7-01: no guard asserts `hash_gpu_material_fields` covers every `GpuMaterial` field — the three existing pins are mutually blind to a field omitted from both hash walks

**Labels**: `medium,renderer,test-gap,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3568 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Material Table
- **Location**: `crates/renderer/src/vulkan/material.rs` (`hash_gpu_material_fields`), `crates/renderer/src/vulkan/context/mod.rs` (`DrawCommand::material_hash`, `material_hash_matches_gpu_material_field_hash`)
- **Status**: OPEN (missing regression guard; today's coverage verified complete)
- **Description**: `MaterialTable::intern_by_hash` keys its `FxHashMap<u64, u32>` dedup index solely on the u64 returned by `hash_gpu_material_fields` / `DrawCommand::material_hash`. A `GpuMaterial` field that is populated by `to_gpu_material` but omitted from **both** hash walks makes two visually-different materials collapse onto one table slot — the first-seen record wins and every later draw renders with the wrong value. Nothing in the test suite can fail on that. The three pins that look like they cover it do not:
  - `gpu_material_size_is_432_bytes` (`material.rs:1494`) pins `size_of`, which a correctly-added-but-unhashed field still satisfies (the author bumps 432 → 436).
  - `gpu_material_glsl_field_order_matches_rust_struct` (`scene_buffer/shader_contract_tests.rs:1383`) compares the Rust struct against `include/bindings.glsl` — both sides get updated in a normal field addition.
  - `material_hash_matches_gpu_material_field_hash` (`context/mod.rs:2638`) compares the two hash walks **against each other**; it passes when a field is missing from both.

  The only live net is the `#[cfg(debug_assertions)]` byte-equality `debug_assert!` inside `intern_by_hash` (`material.rs:1344`), which is runtime-only, debug-only, and fires only if content that differs in the unhashed field is actually loaded that session. Release builds mis-render silently. The struct has grown 272 → 260 → 296 → 300 → 348 → 364 → 396 → 432 B across ~8 separate additions (size history on `GpuMaterial`, `material.rs:40`), so this is a recurring edit path, not a hypothetical one.
- **Evidence**: Mechanical diff of the struct's declared field names against the `mat.<field>` identifiers in the `hash_gpu_material_fields` body: 108 fields declared (108 × 4 B = 432 B, no pad fields), 108 hashed, symmetric difference empty — coverage is complete **today**. `DrawCommand::material_hash` reaches the same 108 via 97 literal `write_u32` calls plus the `for texture_index in &self.supplemental_texture_indices[..12]` loop. `grep -rn "hash_gpu_material_fields"` across `crates/renderer/src` returns no test that enumerates the struct's fields; the only field-specific hash tests are the two single-field pins `material_alpha_participates_in_the_dedup_hash` (`material.rs:1448`) and `greyscale_lut_index_difference_is_distinct` (`material.rs:2063`). `cargo test -p byroredux-renderer --lib material` → 52 passed, 0 failed.
- **Impact**: A future `GpuMaterial` field addition that misses both hash walks silently merges distinct materials in release builds — the failure presents as "some objects render with a neighbour's material", with no log line, no assert, and no failing test. Real interior cells intern 50–200 unique materials and a Skyrim radius-3 grid 4000+, so the collapsed pair is highly likely to be visible.
- **Suggested Fix**: Add a source-scanning test next to `gpu_material_size_is_432_bytes`. The machinery already exists and is already applied to this exact file: `shader_contract_tests.rs:1384` does `include_str!("../material.rs")` and `parse_rust_struct_fields(rust_src, "pub struct GpuMaterial")`; `gpu_instance_layout_tests.rs:180` uses the same helper as a ban-list guard. Parse the struct's field names, extract the `mat.<ident>` identifiers from the `hash_gpu_material_fields` body out of the same `include_str!` source, and assert set equality in both directions (a field in the struct but not the hash = silent dedup collapse; a stale identifier in the hash but not the struct = the walk drifted). Both assertion messages should name the field.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D7-01

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

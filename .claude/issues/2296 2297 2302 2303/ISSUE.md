# #2296 — MAT-D1-NEW-01: no cross-crate assert pins NIF importer's material_kind literals to byroredux_renderer::MATERIAL_KIND_*

**Severity**: LOW · **Domain**: binary (byroredux) — the fix site per the issue is `byroredux/src/material_translate.rs`'s test module (the only crate depending on both `byroredux-nif` and `byroredux-renderer`)
**Location**: `crates/nif/src/import/material/dedicated_shader.rs:336,485`, `crates/nif/src/import/material/legacy_properties.rs:407` (literal `material_kind = 101/102/103`), vs `byroredux_renderer::MATERIAL_KIND_*` (`crates/renderer/src/shader_constants_data.rs:81`)

`crates/nif` has no dependency on `byroredux-renderer`, so nothing pins the importer's raw `material_kind` literals (101/102/103) to the renderer's named `MATERIAL_KIND_*` constants — only literal-to-literal asserts exist inside `crates/nif`'s own tests. A future renumber of the renderer constants would keep `cargo test -p byroredux-nif` green while silently misrouting shading. No live defect — literals currently agree.

**Suggested fix**: a two-line cross-crate assert in `byroredux/src/material_translate.rs`'s test module, pinning the raw literals used at import time to the renderer's named constants.

---

# #2297 — MAT-D1-NEW-02: draw_command_eligible_for_tlas excludes MATERIAL_KIND_EFFECT_SHADER but not MATERIAL_KIND_FIRE_REFRACTION

**Severity**: LOW · **Domain**: renderer (byroredux-renderer)
**Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs:437-441` (`draw_command_eligible_for_tlas`)

`draw_command_eligible_for_tlas` excludes `MATERIAL_KIND_EFFECT_SHADER` from the TLAS but not `MATERIAL_KIND_FIRE_REFRACTION`, despite the latter's own constant doc requiring the same exclusion. A sibling predicate in the same file (`predicates.rs:610`) *does* exclude `MATERIAL_KIND_FIRE_REFRACTION` — confirming the omission is an asymmetry, not a deliberate choice. No live defect today (no producer sets `in_tlas` for fire-refraction draws) — defense-in-depth gap only.

**Suggested fix**: add the same `MATERIAL_KIND_FIRE_REFRACTION` exclusion to `draw_command_eligible_for_tlas`, mirroring the sibling predicate at line 610.

---

# #2302 — NIFAL-D6-08: NiTriStripsData.normals not cross-checked by resolve_tri_strips_data_refs, unlike sibling packed_triangle_winding check

**Severity**: LOW · **Domain**: nif (byroredux-nif)
**Location**: `crates/nif/src/import/collision/shape.rs` (`resolve_tri_strips_data_refs`), vs. `packed_triangle_winding` (`shape.rs:457`, gated for `BhkPackedNiTriStripsShape`)

`NiTriStripsData.normals` is never cross-checked by the `bhkNiTriStripsShape`-derived collision path (`resolve_tri_strips_data_refs`), unlike the sibling `packed_triangle_winding` check that `BhkPackedNiTriStripsShape`'s resolve path already has. Explicitly **not** a fix for #2193 (already hand-verified zero disagreements on its repro content) — documented asymmetry only, no known live trigger.

**Suggested fix**: add the same normal-vs-winding cross-check to `resolve_tri_strips_data_refs` that `packed_triangle_winding` provides for the packed-mesh path.

---

# #2303 — NIFAL-D7-02: nifal.md conflates live AnimatedMorphWeights sink with genuinely-parked ambient colour channels

**Severity**: LOW · **Domain**: docs (no crate — doc-only fix)
**Location**: `docs/engine/nifal.md:244-245`

The doc lumps morph-weight channels in with genuinely-parked per-light ambient channels ("intentionally parked... no renderer consumer yet"). Since `a8b0cf64`, morph-weight channels reach a live `AnimatedMorphWeights` ECS sink every frame (confirmed via `sink_lifecycle_end_to_end_tests`) — they only lack a GPU/mesh-vertex-blend consumer (tracked separately by #2221). Ambient genuinely is still dropped. Doc-only, no behavior impact.

**Suggested fix**: split the sentence — keep ambient colour channels as "intentionally parked, no consumer"; describe morph-weight channels as "reaches `AnimatedMorphWeights` every frame, GPU/mesh-vertex-blend consumer tracked by #2221."

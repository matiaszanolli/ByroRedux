# #3856: TD1-2026-09-05-07: `walk/mod.rs` crossed 2165 production LOC — split the three independent satellite walkers out (note: the SKILL's stated rationale, "per the module doc's own category list", does not exist)

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-07) via `/audit-publish`, 2026-09-05. Labels: `low,nif-parser,nif,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3856 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD1-2026-09-05-07), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/nif/src/import/walk/mod.rs` (2165 production / 2167 total LOC)
- **Status**: NEW
- **Age**: `fc4b3f11` origin; 1399 → 2167 total across 37 commits
- **Description**: The skill directs a split "per the module doc's own category list". **There is no
  category list** — `walk/mod.rs`'s entire module doc is one line, `//! Scene graph walking —
  hierarchical and flat traversal.`, which does not mention the satellite walkers at all. The axis
  is still correct; it just has to be derived from the code, which is what follows. (The stale
  rationale itself belongs to **Dimension 4**.)
- **Evidence**: `walk_node_lights`, `walk_node_texture_effects` and `walk_node_particle_emitters_flat`
  are **not** called from `walk_node_hierarchical` or `walk_node_flat` — they are independent
  entry points invoked only from `crates/nif/src/import/mod.rs` (`:83`, `:495`, `:520`). That makes
  the extraction free of shared-state threading:

  | Proposed file | Symbols | ≈LOC |
  |---|---|---|
  | `walk/mod.rs` | `HierWalkCtx`, `walk_node_hierarchical` (416), `FlatWalkCtx`, `walk_node_flat` (294), `as_ni_node`, `switch_active_children`, `has_live_visibility_controller`, `has_packed_combined_geom_extra`, `MAX_NIF_NODE_DEPTH` | ~880 |
  | `walk/emitter.rs` | `extract_particle_material`, `ParticleMaterial`, `collect_force_fields`, `extract_first_color_curve`, `extract_emitter_params`, `extract_emitter_max_particles`, `extract_emitter_rate` (258), `walk_node_particle_emitters_flat` | ~740 |
  | `walk/lights.rs` | `walk_node_lights`, `imported_light_from_base`, `attenuation_radius` | ~180 |
  | `walk/texture_effect.rs` | `walk_node_texture_effects`, `resolve_affected_node_names`, `resolve_block_ref_names` | ~160 |
  | `walk/node_attrs.rs` | `extract_tree_bones`, `extract_range_kind`, `extract_lod_group`, `extract_bs_value_node`, `extract_bs_ordered_node`, `extract_billboard_mode`, `is_editor_marker` | ~170 |

  Three production functions exceed 200 LOC: `walk_node_hierarchical` (416), `walk_node_flat` (294),
  `extract_emitter_rate` (258 — itself a nest of six inner `fn`s plus an inner `enum CurveTier`, the
  clearest single extraction candidate in the file).
- **Impact**: LOW. This is the healthiest file in the bucket — cohesive, well-commented, and only
  8 % over threshold. Filed for the diff and because the emitter cluster (~34 % of the file) is where
  the per-game particle work keeps landing.
- **Related**: `walk/tests.rs` (61 KB) should be split in the same shape; `/audit-nif` owns correctness.
- **Suggested Fix**: extract `walk/emitter.rs` first (largest, cleanest cut). `pub(super)` symbols
  become `pub(crate)` or get re-exported from `walk::` — no signature changes.
- **Effort**: small

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved

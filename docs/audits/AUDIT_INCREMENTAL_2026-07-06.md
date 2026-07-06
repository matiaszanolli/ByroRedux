# Incremental / Delta Audit — 2026-07-06

**Scope:** last 10 commits (`git diff HEAD~10..HEAD`), `155852e3..d59f40ac`,
plus the working-tree modification to `crates/core/src/ecs/world.rs` noted at
session start (now committed as part of `d4b981fa`, in-range).

**Method:** each changed path routed to its owning audit dimension per
`audit-incremental/SKILL.md` Step 2, then the diff hunks audited against that
dimension's checks (Step 3). Deduped against 36 open issues
(`/tmp/audit/issues.json`) and `docs/audits/`.

---

## 1. Change summary

| Commit | Theme | Risk area |
|--------|-------|-----------|
| `155852e3` | #1885 — route `NiBlendInterpolator` blend-array counts through `allocate_vec` | NIF parser |
| `db121f96` | #1834/#1835 — save `ActorValues` + guard NPC-spawn-stamp save gap | ECS save/load |
| `aedcba12` | #1836/#1837 — name the poisoned lock in `clear_entities` + `insert_resource` | ECS |
| `8b50e238` | #1840/#1841 — delete 7 dead `NifVariant` predicates + regen 5 baselines | NIF parser |
| `a8d65d6c` | #1889 — materialise VWD flag as per-placement `VisibleWhenDistant` marker | ESM / cell loader / EXAL |
| `d4b981fa` | #1890/#1891 — pin the VWD spawn plumbing + document resource poison panics | ECS / cell loader |
| `196169a8` | #1874 — add `DBG_VIZ_MOTION` debug view (ghosting root-cause) | Renderer (shader) |
| `42adc1e6` | #1892/#1893 — sync stale RT/denoiser docs with live pipeline | docs |
| `e4d574dc` | #1894/#1895 — correct stale depth + SVGF docstring facts | docs |
| `d59f40ac` | add 2026-07-05 audit reports | docs |

Themes: **hardening + doc-sync**, not new feature work. Three defensive
fixes (poison naming, allocation budget guard, save-registry gap), one
dead-code sweep, one diagnostic-only shader debug view, the rest docstring
corrections and audit-report drops.

---

## 2. Routing map

| Changed file | Dimension(s) | Result |
|--------------|-------------|--------|
| `crates/core/src/ecs/world.rs` | `/audit-ecs`, `/audit-concurrency` | clean |
| `crates/core/src/ecs/world_tests.rs` | `/audit-regression` | clean (new coverage) |
| `crates/core/src/ecs/components/actor_values.rs` | `/audit-ecs`, `/audit-save` | clean |
| `byroredux/src/save_io.rs` | `/audit-save` | clean (new coverage) |
| `crates/save/src/registry.rs` | `/audit-save` | clean |
| `crates/nif/src/blocks/interpolator.rs` | `/audit-nif` | clean |
| `crates/nif/src/blocks/interpolator_tests.rs` | `/audit-regression` | clean (new coverage) |
| `crates/nif/src/version.rs` | `/audit-nif` | clean (dead-code delete, no call sites) |
| `crates/nif/src/blocks/{dispatch_tests/nodes,tri_shape_*_tests,collision/shape_compound_tests}.rs` | `/audit-regression` | clean (test adaptation) |
| `crates/nif/tests/data/per_block_baselines/*.tsv` | `/audit-regression` | clean (baseline regen) |
| `byroredux/src/components.rs` | `/audit-ecs` | clean (new marker component) |
| `byroredux/src/cell_loader/references/mod.rs` | per-game, `/audit-legacy-compat` | clean (new coverage) |
| `byroredux/src/cell_loader/object_lod.rs` | per-game | clean (comment only) |
| `crates/plugin/src/esm/cell/{mod,support}.rs` | per-game, `/audit-legacy-compat` | clean |
| `crates/plugin/src/esm/records/grup_walker.rs` | per-game | clean |
| `crates/plugin/src/esm/cell/tests/*.rs` | `/audit-regression` | clean (new coverage) |
| `crates/renderer/shaders/triangle.frag` (+ `.spv`) | `/audit-renderer` | clean (diagnostic-only) |
| `crates/renderer/shaders/include/shader_constants.glsl`, `src/shader_constants_data.rs`, `build.rs` | `/audit-renderer` (GPU-const lockstep) | clean (values consistent) |
| `crates/renderer/src/vulkan/svgf.rs` | `/audit-renderer` | clean (docstring matches live code) |
| `crates/renderer/src/vulkan/acceleration/constants.rs` | `/audit-renderer` | clean (docstring only) |
| `crates/bsa/examples/obl_sweep.rs` | tooling | out of scope (throwaway example) |
| `.claude/issues/**`, `docs/**` | `/audit-tech-debt` | doc — no code impact |

---

## 3. Findings

**No new bugs or regressions.** Every changed code path was re-read with
minimal surrounding context and could not be shown to introduce a defect.

### Premises checked and disproved (recurring stale-premise guard)

- **`version.rs` predicate deletion leaves live call sites** — DISPROVED.
  Grepped all 7 deleted predicates (`has_material_crc`, `has_properties_list`,
  `avobject_flags_u32`, `has_shader_alpha_refs`, `has_effects_list`,
  `uses_bs_tri_shape`, `has_culling_mode`) workspace-wide. Every remaining hit
  is in a `//` comment (base.rs / node.rs / shader.rs raw-bsver rationale, and
  test explanations), not a call expression. The one surviving predicate
  (`has_shader_property_fo3_fields`) still has a live consumer in
  `shader_flags.rs`. Deletion is safe.

- **`build_static_object_from_subs` signature change breaks an unupdated caller**
  — DISPROVED. The `visible_when_distant: bool` parameter was threaded to all 5
  call sites (4 in `support.rs`, 1 in `grup_walker.rs`), and all 4
  `StaticObject { … }` literal constructions gained the new field. No orphan
  construction site exists.

- **`interpolator.rs` `items = allocate_vec(...)` reassignment discards data**
  — DISPROVED. In `parse_modern` the reassignment target was `Vec::new()`
  (empty, manager-controlled blends carry no array); `allocate_vec` returns an
  empty capacity-reserved `Vec` (`stream.rs:265`), so the subsequent
  `for … items.push(…)` loop fills it correctly. The legacy path is a fresh
  binding. Covered by the new `parse_legacy_blend_interpolator_rejects_oversized_array_size` test.

- **`ActorValues` serde derive gated behind `feature = "inspect"` won't compile
  on the save path** — DISPROVED. `crates/core/Cargo.toml` defines
  `save = ["inspect"]`, and `SaveRegistry::register_component` bounds
  `T: Serialize + DeserializeOwned`. `ActorValues`/`ActorValue` follow the exact
  same `#[cfg_attr(feature = "inspect", derive(Serialize, Deserialize))]` pattern
  as every other saved component (material.rs, light.rs, name.rs, …). The
  `actor_values_survive_save_load_round_trip` test compiles and exercises it.

- **`world.rs` `insert_resource` downcast `.expect()` can panic on a valid
  replace** — DISPROVED. The resources map is keyed by `TypeId::of::<R>()`, so
  the stored `Box<dyn Any>` for that key is always `Box<R>`; the downcast cannot
  fail. The new poison-panic behavior (was `.ok()`→`None`) is the intentional
  #466 fail-fast doctrine, documented on the method and mirroring
  `remove_resource`.

- **`svgf.rs` doc-sync introduced reverse doc-rot** — DISPROVED. The docstring
  now states `indirect_history` is `B10G11R11_UFLOAT_PACK32`, which matches
  `INDIRECT_HIST_FORMAT` (svgf.rs:114); the à-trous claim matches
  `ATROUS_ITERATIONS = 5` (svgf.rs:91). Docs corrected toward live code.

- **`DBG_VIZ_MOTION` constant drifts between Rust and GLSL** — DISPROVED.
  Rust `DBG_VIZ_MOTION = 0x20000` (131072) == GLSL `#define DBG_VIZ_MOTION 131072u`,
  and `build.rs` emits the define. The view is gated entirely behind the debug
  bit with an early return; no effect on normal rendering.

---

## 4. Missing tests

None. The delta is well-covered:

- #1834/#1835 — `actor_values_survive_save_load_round_trip` +
  `npc_spawn_stamped_components_are_saved_or_intentionally_rederived` (the
  structural tripwire so the save-gap class can't silently recur).
- #1885 — `parse_legacy_blend_interpolator_rejects_oversized_array_size`.
- #1889/#1890 — `stamp_visible_when_distant_marks_only_flagged_roots`
  (spawn half) + the `esm/cell/tests/addn_stat.rs` record→flag pin.
- #1836/#1837 — `world_tests.rs` (+80 lines) poison-naming coverage.

The `DBG_VIZ_MOTION` shader debug view has no automated test, but shader debug
paths require a Vulkan device and are out of `cargo test` scope
(`_audit-common.md` smoke-test tier); this is not a coverage gap.

---

## Recommendation

Clean delta — no publishable findings. If you want a record of the pass:

```
/audit-publish docs/audits/AUDIT_INCREMENTAL_2026-07-06.md
```

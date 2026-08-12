# PERF-DOC: performance documentation & skill-text drift (3 sites)

**Issue**: #2691
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`

Consolidated documentation / skill-text drift from the `/audit-performance` half of `/audit-suite renderer-deep` (2026-08-12). Filed as one issue rather than one-per-site — they share a root cause.

Two of these are **corrections to beliefs recorded elsewhere**, not just stale prose:
- **PERF-D2-01** — the Dimension-2 checklist still describes the pre-#2165 `z_write` form of `needs_two_sided_blend_split`. The live predicate is `is_blend && two_sided && order_dependent_glass`. The stale text *inverts* the regression test's meaning. Independently found by REN-D12-01 in the renderer audit.
- **PERF-D2-02** — the split's dormancy is **structural, not empirical**: `collect_static_mesh_draws` force-clears `two_sided` for `MATERIAL_KIND_GLASS`, so only kind-11 MultiLayerParallax can ever reach the predicate. Prior notes recorded this as an empirical observation across tested cells; the real cause is upstream and deterministic, which changes what a fix must touch.
- **PERF-D2-03** — `DRAW_SORT_PARALLEL_THRESHOLD = 3000` is **well-placed** (the in-comment crossover table could not be disproved); only its stated rationale is wrong. The "typical Bethesda cell = 400-1500" band is contradicted by the repo's own runtime baselines (324 / 1839 / 2342 / 2553 / 3440). The same figure also appears in the audit skill text, so it is propagating.

---

## PERF-D2-01

- **Severity**: LOW
- **Dimension**: 2 — Draw & Instancing
- **Location**: `.claude/commands/audit-performance/SKILL.md:91` (Dimension 2 checklist, "Two-sided blend split gate (#1804)")
- **Status**: NEW
- **Description**: The skill text instructs auditors that
  `needs_two_sided_blend_split(&DrawBatch)` "requires `z_write` in addition to
  `is_blend && two_sided`", and frames a split on a non-depth-writing batch as the
  regression to look for. The live predicate has not had a `z_write` limb since
  #2165: it is `is_blend && b.two_sided && b.order_dependent_glass`. The `z_write`
  proxy was removed *deliberately* — FO4 BGEM glass is commonly authored
  `z_write == false`, so the old spelling excluded the population the split exists
  for. An auditor following the skill literally would report the correct current
  code as a regression.
- **Evidence**: [draw.rs](crates/renderer/src/vulkan/context/draw.rs):
  ```rust
  pub(super) fn needs_two_sided_blend_split(b: &DrawBatch) -> bool {
      let is_blend = matches!(b.pipeline_key, PipelineKey::Blended { .. });
      is_blend && b.two_sided && b.order_dependent_glass
  }
  ```
  The doc comment above it states the history explicitly ("Both earlier spellings
  were wrong in opposite directions"), and `DrawBatch::order_dependent_glass`'s own
  doc says "The material kind is the real signal; depth state never was."
- **Impact**: Documentation only — but it is the kind of drift that manufactures a
  false-positive finding in every subsequent Dimension-2 run, which is precisely the
  noise class the audit-hygiene rules exist to suppress.
- **Related**: #1804, #2165, `8e55a714`, #2215. **Cross-audit**: independently found
  as REN-D12-01 in the renderer audit.
- **Suggested Fix**: Update the Dimension 2 checklist bullet to the live predicate
  and re-point the "regression to watch for" at a split reappearing on
  non-`order_dependent_glass` batches (the #2165 particle case), not on
  non-`z_write` ones.

---


---

## PERF-D2-02

- **Severity**: LOW
- **Dimension**: 2 — Draw & Instancing
- **Location**: [static_meshes.rs](byroredux/src/render/static_meshes.rs) — `collect_static_mesh_draws`, the glass single-sided override (~lines 448-452); consumed by `needs_two_sided_blend_split` / `is_refractive_glass` in [draw.rs](crates/renderer/src/vulkan/context/draw.rs)
- **Status**: NEW (mechanism); the dormancy itself is already recorded empirically in `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`'s header
- **Description**: The FNV baseline header and prior audit notes record that
  `blended && two_sided == 0` on every measured cell, and correctly warn that
  changes to `needs_two_sided_blend_split` are runtime no-ops. That is presented as
  an observation. It is in fact a **structural guarantee** for the predicate's
  primary target population, and the guarantee is not documented at either site.
  `is_refractive_glass` accepts two signals: `material_kind == MATERIAL_KIND_GLASS`,
  and `material_kind == 11` (MultiLayerParallax) with a non-zero refraction scale.
  But `collect_static_mesh_draws` — the only producer of glass `DrawCommand`s —
  unconditionally clears `two_sided` for `MATERIAL_KIND_GLASS` *before* the
  `DrawCommand` is constructed. So `b.two_sided` is false for every glass batch by
  construction, and `is_blend && two_sided && order_dependent_glass` can only ever be
  satisfied by an alpha-blended, two-sided, kind-11 MultiLayerParallax draw with
  `multi_layer_refraction_scale > 0` — a vanishingly rare Skyrim+ authoring case.
- **Evidence**:
  ```rust
  // render/static_meshes.rs — the only site that sets two_sided on a glass draw
  let two_sided = if material_kind == byroredux_renderer::MATERIAL_KIND_GLASS {
      false
  } else {
      two_sided
  };
  ```
  The other two `DrawCommand` producers cannot reach the predicate either:
  `render::particles::emit_particles` hardcodes
  `material_kind: MATERIAL_KIND_EFFECT_SHADER` (101, rejected by
  `is_refractive_glass` — this is #2165 working as intended), and
  `render::water::reemit_water_planes` only flips `is_water` on an
  already-emitted command, which `draw.rs` excludes from batch formation via
  `skip_batch`.
- **Impact**: No runtime cost — the dead path costs nothing. The impact is
  interpretive: the split is carried as a live mitigation for the #1804/#2237 glass
  compositing artifact, when for engine-classified glass that artifact is actually
  handled by the single-sided override (which solves it by removing back faces
  entirely, at the documented cost of glass interiors not rendering). Two
  independent mitigations for one artifact, one of them unreachable, with neither
  site cross-referencing the other. This also means Dimension 2's split-related
  checklist items are unfalsifiable on real content and should not be used to
  attribute batch-count movement — consistent with the RT-1 / #2215 conclusion that
  the depth-primary alpha-over sort, not this predicate, drove the
  `bench_draws_batches` rise.
- **Related**: #1804, #2165, #2215, #2237; the `two_sided_blend_split_dormant` note.
- **Suggested Fix**: No code change. Add a cross-reference from
  `needs_two_sided_blend_split`'s doc comment to the `MATERIAL_KIND_GLASS`
  single-sided override in `static_meshes.rs`, stating that the glass arm of
  `is_refractive_glass` is unreachable through `b.two_sided` and that kind-11 is the
  only live population. That converts a repeatedly-rediscovered empirical surprise
  into a stated invariant.

---


---

## PERF-D2-03

- **Severity**: LOW
- **Dimension**: 2 — Draw & Instancing
- **Location**: [mod.rs](byroredux/src/render/mod.rs) — `sort_draw_commands` (`DRAW_SORT_PARALLEL_THRESHOLD`) and the rationale comment in `build_render_data` immediately above the `sort_draw_commands` call
- **Status**: NEW
- **Description**: **The constant itself checks out** — I set out to show 3000 was
  misplaced and could not. The in-comment crossover table (re-measured 2026-07-25 on
  a 7950X after `883f57cd` widened the key to 11 tuples) shows serial ~19% ahead at
  N=2000, still ahead at N=2750, tied at N=3000, and parallel pulling away from
  N=5000. 3000 is the first size where the two are interchangeable, which is the
  right place for the gate. What is stale is the *justification prose* wrapped
  around it: "Typical Bethesda cell counts sit in 400–1500 (Prospector ~811,
  GSDocMitchell ~263, exterior radius-3 grid ~1200), so serial remains the common
  path either way; this only moves the 2000–3000 band."
- **Evidence**: `bench_draws_cmds` from the five checked-in runtime baselines in
  `.claude/audit-baselines/runtime/` (regenerated 2026-06-14 → 2026-08-06):

  | baseline cell | `entities_total` | `bench_draws_cmds` | `bench_draws_batches` | `bench_draws_gpu_calls` |
  |---|---:|---:|---:|---:|
  | `oblivion-ICMarketDistrictTheGildedCarafe` | 701 | 324 | 47 | 4 |
  | `fo3-MegatonPlayerHouse` | 3311 | 1839 | 96 | 9 |
  | `skyrim_se-WhiterunDragonsreach` | 8126 | 2342 | 9 | 2 |
  | `fnv-FreesideAtomicWrangler` | 9271 | 2553 | 89 | 25 |
  | `fo4-InstituteBioScience` | 12448 | 3440 | 753 | 42 |

  Exactly one of five sits inside the quoted 400–1500 band. Three sit in the
  1800–2600 range the comment dismisses as merely "the band this moves", and one is
  *above* the gate — `fo4-InstituteBioScience` at 3440 commands takes the parallel
  path (modulo the in-raster prefix split), which the prose says is uncommon.
- **Impact**: No runtime defect. The risk is that the next person tuning this
  constant reasons from the stale band and lowers the gate to "cover typical cells",
  landing back in the 2000–2750 range where the same comment's measured table shows
  serial winning by ~8-24%. Reported so the rationale and the constant stop
  disagreeing.
- **Related**: #934 / PERF-DC-01, #2173, `883f57cd`; reproduction harness
  `manual_bench_draw_sort_serial_vs_parallel` in
  `byroredux/src/render/draw_sort_key_tests.rs` (`--ignored`).
- **Suggested Fix**: Replace the cited cell counts with the current
  `.claude/audit-baselines/runtime/*.tsv` `bench_draws_cmds` column (or reference the
  directory rather than transcribing numbers, per the audit's own cite-don't-copy
  rule), and restate the conclusion as "one of five baseline cells currently crosses
  the gate" rather than "serial remains the common path either way".

---


---

*Filed from [`docs/audits/AUDIT_PERFORMANCE_2026-08-12.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-12.md).*

## Completeness Checks
- [ ] **SIBLING**: Every listed site corrected, incl. the duplicated "400-1500" figure wherever it appears
- [ ] **TESTS**: Where a doc pins a numeric contract, a test or baseline asserts the number

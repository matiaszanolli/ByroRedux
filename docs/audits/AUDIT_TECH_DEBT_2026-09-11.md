# Tech-Debt Audit — 2026-09-11

**Scope**: all 9 dimensions, `--depth deep`. Nine dimension agents, each report
verified against the live tree before merge.
**Commit at audit time**: `b3db49fa` (branch `main`).
**Dedup baseline**: 500 `tech-debt`-labelled issues (9 open) + the prior
`AUDIT_TECH_DEBT_*` reports, most recently 2026-09-05.

## Executive Summary

**20 findings — 0 CRITICAL, 0 HIGH, 6 MEDIUM, 14 LOW.**
Effort: 12 trivial · 6 small · 2 medium · 0 large.

The engine's own hygiene continues to hold: `unimplemented!`/`todo!()` stays at
**0**, live TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE markers stay at **0** (all 22
raw hits across `crates`/`byroredux`/`tools` are documented exclusions —
protocol tags, upstream-reference FIXME citations, or retrospective prose),
`cargo machete` reports zero unused dependencies, and every `#[ignore]` test
(188 sites) still carries a reason string with zero bare-form regressions
since #3749.

This sweep's real news is in two places:

- **Dimension 6 (Stub & Placeholder Implementations) turned up four genuine
  MEDIUM findings** — parsed-but-never-consumed data reachable from shipped
  CLI paths: four Havok constraint types with no CInfo decode (TD6-001),
  `NiStencilProperty` state the renderer never reads (TD6-002), IMGS/IMAD
  HDR tonemap records that are indexed on every ESM parse and never read again
  (TD6-003), and legacy mesh-driven water shader flags with no renderer
  dispatch (TD6-004). None of these are new bugs — each is an already-tracked,
  deliberately-staged gap — but none had previously been surfaced together as
  a *pattern* (import/parse boundary complete, render/consumer boundary absent).
- **The `GpuMaterial` struct-size doc-rot class recurred a fourth time**
  (TD3-001): `#3909` shrank it 432→428 B (removing the unsampled
  `texture_index` lane) and the pinning test + shader-side docs were updated,
  but ~24 sites across 8 files (`material.rs`, `material_tests.rs`,
  `scene_buffer/constants.rs`, `material_translate.rs`, two skill files, six
  `docs/engine/*.md` files) still assert 432 B or the dead test name
  `gpu_material_size_is_432_bytes`. This is independently corroborated by
  `docs/audits/AUDIT_RENDERER_2026-09-11.md` (same day, untracked in git
  status) under IDs `REN-2026-09-11-D3-01`/`D7-01`. Recommend the structural
  fix this time (a `collect_stale_gpu_material_size_claims` source-scan test,
  mirroring the sibling `classify_pbr` self-enforcing test at
  `byroredux/src/workspace_hygiene_tests.rs`) rather than a fifth manual sweep.

**The audit machinery itself is in noticeably better shape than the last two
cycles.** The path-validation gate (`_audit-validate.sh`) reports **zero
STALE refs** across 2580 checked paths — the first clean run in recent memory
— and the shader `#define` provenance gate that Dimension 7 flagged as blind
to `shaders/include/` on 2026-09-05 (TD7-02) is confirmed fixed (`#3880`/`#3815`).
Dimension 4's findings are now three trivial doc-pointer misses in *other*
skills (a renamed test helper, an unsplit `boot.rs` reference), not systemic
rot.

**Dimension 1's primary bucket held flat at 2 files** —
`crates/renderer/src/vulkan/context/mod.rs` (2831 prod LOC, unchanged) and
`crates/sdk/src/compatibility/storage_util.rs` (2160 prod LOC, unchanged) —
confirming last cycle's numbers rather than finding fresh growth. Both get a
concrete split-axis proposal below (TD1-001, TD1-002) rather than a re-flag.

### Delta vs the 2026-09-05 baseline

| Metric | 2026-09-05 | 2026-09-11 | Note |
|---|---|---|---|
| Markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE) | 20 | 22 | still 0 live; delta is new documented-exclusion prose, not new debt |
| `allow(dead_code)` | 43 | 29 | −14; largely explained by young-crate splits (#3843/#3851-3855) removing scaffolding that was never wired, plus two stale annotations found this cycle (TD8-001/002) still pending removal |
| `unimplemented!` / `todo!()` | 0 | 0 | unchanged |
| `#[ignore]` (all forms) | 181 (crates+byroredux) | 189 | growth from new tests; bare form still 0 |
| Files >2000 **production** LOC | 12 | **2** | −10; ten left the bucket via real splits since 2026-09-05 (#3843 `extensions.rs`, #3854 `fragment.rs`, #3851 `compatibility.rs`, #3852 `papyrus_provider.rs`, #3853 `runtime.rs`, #3737 `texture_registry.rs`, #2256 `volumetrics.rs` mostly, #3855 `boot.rs`, #3857 `material.rs`, `9aae918b` `import/walk/mod.rs`) — see Dimension 1 below; none re-proposed |
| Total LOC >2000 (secondary bucket) | 40 | 39 | roughly flat; still lower priority, none escalate to primary |
| Open tech-debt-labelled issues | 12 | 9 | 3 closed net of any new filings since |

## Baseline Snapshot

Re-runnable counts, for the next audit to diff against:

```
markers (TODO/FIXME/HACK/XXX/TBD/WIP/KLUDGE): 22   (0 live; all documented exclusions)
allow(dead_code):                              29
unimplemented!/todo!():                         0
#[ignore] tests (crates+byroredux+tools):     189   (bare form: 0)
production LOC >2000:                           2
total LOC >2000 (secondary bucket):            39
open tech-debt-labelled issues:                 9
```

Primary bucket (production LOC), both re-verified exact matches to the prior
cycle's figures: `crates/renderer/src/vulkan/context/mod.rs` **2831** ·
`crates/sdk/src/compatibility/storage_util.rs` **2160**.

Ten files left the primary bucket since 2026-09-05 via real splits (#3843,
#3854, #3851, #3852, #3853, #3737, #2256, #3855, #3857, `9aae918b`) — do not
re-propose any of them; see Dimension 1 discussion.

## Top 10 Quick Wins (trivial/small effort)

1. **TD8-001** — delete the stale `#[cfg_attr(not(test), allow(dead_code))]`
   on `ActionState::was_released` (`byroredux/src/interaction.rs:692`); it has
   a real production caller now.
2. **TD8-002** — remove three stale `#[allow(dead_code)]` in
   `byroredux/src/groundcover_translate.rs` (82, 91, 172); `#4054` gave
   `layer_affinity` a live CPU-side consumer.
3. **TD4-001** — fix `audit-esm/SKILL.md:276`'s backticked dead test-helper
   name (`record_parsers_with_embedded_form_ids_take_a_remap`); the paragraph
   already names its replacement.
4. **TD4-002** — update `audit-ecs/SKILL.md:305`'s bare `boot.rs` pointer to
   the post-#3855 path, mirroring the already-correct `audit-fnv/SKILL.md:188`.
5. **TD4-003** — italicize (don't backtick) the historical `boot.rs` mention
   in `audit-fnv/SKILL.md:188` per the path gate's own convention.
6. **TD2-001** — replace `ssao.rs`'s hand-rolled post-dispatch image barrier
   with the existing `descriptors::image_barrier_general_to_shader_read`
   helper it already imports siblings of.
7. **TD6-006** — fix `PerkRecord`'s stale doc comment in
   `crates/plugin/src/esm/records/misc/magic.rs:220-226`: CTDA/EPFD decode are
   already implemented, not "follow-ups."
8. **TD1-001 (partial)** — extract `context/mod.rs`'s 5 `fill_*` telemetry
   methods into a new `mod telemetry;` (~392 lines), one of two independent
   moves that together drop the file under 2000 prod LOC.
9. **TD2-003** — drop the redundant `b"VMAD" => has_script = true` arm in
   `scol.rs`'s second sub-record loop; use the `has_script` already computed
   by `CommonNamedFields::from_subs_with_remap`.
10. **TD3-001 (mechanical portion)** — find/replace `432`→`428` and
    `gpu_material_size_is_432_bytes`→`gpu_material_size_is_428_bytes` across
    the ~24 sites enumerated in the finding; fix the one genuine arithmetic
    error (`material.rs:392`, `424+4=428` not `432`) alongside it.

## Top 5 Medium Investments

1. **TD6-001 through TD6-004 (as a set)** — four import/parse-complete,
   render/consumer-absent gaps (Havok constraint CInfo, `NiStencilProperty`,
   IMGS/IMAD HDR tonemap, legacy mesh-water shader flags). Each is individually
   MEDIUM and separately tracked; treating them as one investment theme
   ("audit every ESM/NIF field that reaches an `EsmIndex`/`MaterialInfo` but
   has zero downstream reader") would likely surface more of the same class
   before it recurs a third time.
2. **TD1-002** — split `storage_util.rs`'s 384-line, 17-arm
   `adapt_storage_util_global_list` into per-verb helpers behind a thin
   dispatcher, and convert the adjacent 251-line `papyrus_storage_util_declarations`
   `vec![...]` literal into a data table — the file's own 2026-09-09 commit
   already proved this exact pattern works on a sibling 106-arm match.
3. **TD1-003** — split `crates/plugin/src/esm/records/mod.rs`'s parsing entry
   point (`parse_esm`, `parse_esm_with_load_order`, `character_rules_profile`)
   out to `records/parse.rs`, restoring `mod.rs` to a pure re-export barrel.
4. **TD3-001 structural fix** — add `collect_stale_gpu_material_size_claims`
   to `byroredux/src/workspace_hygiene_tests.rs`, mirroring the existing
   `classify_pbr` self-enforcing test, so the fourth recurrence of this exact
   doc-rot class becomes the last one.
5. **TD2-002** — add `image_barrier_undef_to_shader_read` (and a `_layers`
   variant of `image_barrier_undef_to_general`) to `descriptors.rs`, then
   migrate `gbuffer.rs`, `frame_upscaler.rs`, and `caustic.rs`'s three
   independently-hand-rolled init barriers onto it.

## Findings

### MEDIUM

#### TD3-001: `GpuMaterial` 432→428 B shrink (#3909) never propagated past the pinning test and shader docs
**Dimension**: 3 (Stale Documentation & Comments)
**Severity**: MEDIUM (struct-size doc-comment drift promotion, per severity table; independently corroborated same-day by `docs/audits/AUDIT_RENDERER_2026-09-11.md` REN-2026-09-11-D3-01/D7-01)
**File**: `crates/renderer/src/vulkan/material.rs` (7 sites: 43, 48-49, 1047 ×2, 1321, 1363, 1380, 392), `material_tests.rs` (56, 1284), `scene_buffer/constants.rs` (174, 177), `shader_contract_tests.rs:2214`, `byroredux/src/material_translate.rs` (104, 107), `byroredux/src/render/static_meshes.rs:1108`, `.claude/commands/audit-safety/SKILL.md` (261, 269, 282), `.claude/commands/_audit-common.md:101`, `docs/engine/material-abstraction.md:27`, `docs/engine/nifal.md:85`, `docs/engine/memory-budget.md:95`, `docs/engine/shader-pipeline.md` (370, 493), `docs/engine/rt-lighting-material-recovery.md` (38, 115, 641), `docs/engine/renderer.md` (136, 539)
**Effort**: small
**Finding**: `GpuMaterial` is pinned at 428 B by `crates/renderer/src/vulkan/material_tests.rs:63`'s `gpu_material_size_is_428_bytes` (107 flat scalar fields × 4 B). The shader side (`bindings.glsl`, `triangle.frag`) already documents this correctly, including the `#3909` shrink narrative. ~24 sites across the Rust doc comments, two test-file doc comments, two skill files, and six `docs/engine/*.md` files still assert 432 B and/or cite the dead test name `gpu_material_size_is_432_bytes`. One site (`material.rs:392`) has an independent arithmetic error: `// offset 424 → total 432` should read 428 regardless of the historical narrative. `material.rs:1047`'s "108 live scalar fields" is also one off (current count: 107). This is the third or fourth recurrence of this exact class for this struct (`#1321`/`#1755`, `#3846`, `#3414`/`#3240`).
**Proposed fix**: Mechanical find/replace at every site (`432`→`428`, test name update, MB-derived figures in `constants.rs:174` and `memory-budget.md:95`), fix the two numeric errors, extend each "growth chain" narrative with the `#3909` link, and add a `collect_stale_gpu_material_size_claims` source-scanning test to `byroredux/src/workspace_hygiene_tests.rs` (mirroring the existing `classify_pbr` self-enforcing test) so a fifth recurrence fails the build instead of requiring another manual sweep.

#### TD6-001: Four Havok constraint block types are base-only stubs (no CInfo decode)
**Dimension**: 6 (Stub & Placeholder Implementations)
**Severity**: MEDIUM (reachable from shipped CLI — any NIF/cell load touching an actor skeleton)
**File**: `crates/nif/src/lib.rs:189` (`is_havok_constraint_stub`), consumed at `:499`; dispatch in `crates/nif/src/blocks/mod.rs`; telemetry field `crates/nif/src/scene.rs:108` (`stubbed_drift_histogram`)
**Effort**: large (needs CInfo layout for each of 4 types, verified against nif.xml + Havok source)
**Finding**: `bhkBallAndSocketConstraint`, `bhkStiffSpringConstraint`, `bhkGenericConstraint`, and `bhkBallSocketConstraintChain` (#979) fall through to the base `NiTimeController`-shaped stub — CInfo fields never decoded, only skipped via block-size recovery. Deliberately tracked (#117, #979) and kept out of the main `drift_histogram` via a parallel counter specifically so it doesn't drown real parser-drift signal. Genuinely still open; affects Oblivion→Skyrim-era ragdolls/physics props where PHYSAL is otherwise "converged."
**Proposed fix**: Implement the four CInfo decoders (mirroring existing `bhkHingeConstraint`/`bhkRagdollConstraint` parsers if present), or explicitly re-confirm in ROADMAP.md/PHYSAL docs that this is a known permanent gap with an issue link.

#### TD6-002: `NiStencilProperty` state captured at parse time, never consumed by the renderer
**Dimension**: 6 (Stub & Placeholder Implementations)
**Severity**: MEDIUM (reachable from shipped CLI — any NIF with a stencil property)
**File**: `crates/nif/src/import/material/mod.rs:1732` (cross-reference); `MaterialInfo.stencil_state`; consumer gap `crates/renderer/src/vulkan/pipeline.rs:449-457`
**Effort**: medium
**Finding**: The importer decodes and stores all 7 non-`draw_mode` `NiStencilProperty` fields (fixed under #337 so nothing drops at the import boundary), but `pipeline.rs:449` hardcodes `stencil_test_enable(false)` unconditionally — comment explicitly states the data is "dormant until per-material stencil pipeline variants land." `grep -rn stencil_state crates/renderer` confirms zero non-comment reads outside `pipeline.rs`'s own note.
**Proposed fix**: Implement the stencil pipeline variant (tracked as #337 follow-up), or downgrade the "dormant" framing to an explicit ROADMAP known-gap entry.

#### TD6-003: IMGS (image-space/HDR tonemap) records parsed but have zero downstream consumer
**Dimension**: 6 (Stub & Placeholder Implementations)
**Severity**: MEDIUM (reachable from shipped CLI — any `--esm` load populates the map; visually invisible until compared against a reference)
**File**: `crates/plugin/src/esm/records/misc/world.rs:1358` (`ImgsRecord`), `:1372` (`parse_imgs`); stored via `dispatch_misc_gameplay_a.rs:118` into `EsmIndex.image_spaces` (`index.rs:185`)
**Effort**: large (full DNAM decode + IMAD modifier-graph parser + render-side consumer + CELL/WRLD binding)
**Finding**: `parse_imgs` intentionally only captures `EDID` + raw `DNAM` bytes ("deferred to M48" per its own comment). `grep -rn image_spaces crates byroredux` shows the map is populated on every ESM parse and never read again — no cell-loader binding, no render-pass consumer. The entire per-region/per-cell HDR/tint/tonemap feature currently has zero rendering effect despite full indexing. The "deferred to M48" framing is stale — M48 shipped (Scaleform UI route) and didn't touch this.
**Proposed fix**: Land the DNAM decode + a render-side consumer (tracked as SK-D6-NEW-03/#624), or correct the stale "deferred to M48" comment and link a fresh tracking issue.

#### TD6-004: Legacy mesh-driven water shader flags parsed but not dispatched by the renderer
**Dimension**: 6 (Stub & Placeholder Implementations)
**Severity**: MEDIUM (reachable from shipped CLI — Oblivion/Skyrim rivers and most `meshes/water/*.nif` ship `BSWaterShaderProperty`)
**File**: `crates/nif/src/import/material/mod.rs:918-946` (`water_shader_flags`); threaded via `byroredux/src/material_translate.rs`; consumer gap in `byroredux/src/render/water.rs` / `crates/renderer/src/vulkan/water.rs`
**Effort**: medium
**Finding**: `BSWaterShaderProperty.water_shader_flags` (Displacement/LOD/Depth/Reflections/Refractions/Cubemap, per nif.xml) is captured end-to-end into the `Material` ECS component and versioned in the save format, but `grep -rn water_shader_flags byroredux crates/renderer` shows no read outside `material_translate.rs`'s own tests. Doc comment says renderer dispatch is "#977 follow-up work," still not wired. Distinct from the WATR-record-driven water system (audited under WATAL) — this is specifically NIF-mesh-authored water common on Oblivion/Skyrim exteriors.
**Proposed fix**: Confirm current visual behavior (fixed-default feature set regardless of authored flags would make this fidelity-only, not correctness), then either wire the per-flag dispatch (#977) or document the fixed-default behavior explicitly in `docs/engine/watal.md`.

#### TD7-001: `BLOOM_BYTES_PER_PIXEL_X1024`'s geometric-series literal is not derived from or tested against `BLOOM_MIP_COUNT`
**Dimension**: 7 (Magic Numbers & Hardcoded Constants)
**Severity**: MEDIUM
**File**: `crates/renderer/src/vulkan/bloom.rs:91-92`
**Effort**: small
**Finding**: `pub const BLOOM_BYTES_PER_PIXEL_X1024: u32 = (341 + 340) * 4 * super::sync::MAX_FRAMES_IN_FLIGHT as u32;` — the `341`/`340` are a hand-computed geometric sum correct only for the current `BLOOM_MIP_COUNT = 5` (declared two lines below). Unlike its sibling `CAUSTIC_BYTES_PER_PIXEL` in `caustic.rs` (explicitly derived from live constants and pinned by a test per `#2679`), this constant bakes the mip-count-dependent result in as a bare literal with no test — `grep` confirms no test in `bloom.rs` references it. Feeds `acceleration/predicates.rs`'s VRAM reservation-floor computation; a future `BLOOM_MIP_COUNT` bump would silently mis-report VRAM headroom rather than failing a test.
**Proposed fix**: Either replace the literal with a `const fn` deriving the geometric sum from `BLOOM_MIP_COUNT`, or keep the literal but add a test recomputing the series from `BLOOM_MIP_COUNT` and asserting equality, mirroring `caustic.rs`'s `#2679` precedent.

### LOW

#### TD1-001: `context/mod.rs` still mixes telemetry-fill methods and pure data types with its lifecycle-phase split axis
**Dimension**: 1 (File / Function / Module Complexity)
**Severity**: LOW
**File**: `crates/renderer/src/vulkan/context/mod.rs:2309-2701` (telemetry fillers), `:430-1202` (data types)
**Effort**: small
**Finding**: prod_loc confirmed unchanged at exactly 2831. Two concerns don't fit the existing 17-submodule lifecycle-phase split: (1) 5 telemetry query methods (`fill_upscaler_telemetry`, `fill_scratch_telemetry`, `fill_skin_coverage_stats`, `fill_rt_integrity_stats`, `fill_shadow_mask_census`, ~392 lines) that read internal state into debug-UI/ECS stats structs, none touching draw/init/resize logic; (2) pure data types (`DrawCommand::to_gpu_material`/`material_hash`, `SkyWeatherParams`/`DofView`/`SkyParams` defaults, `ScreenshotHandle`, ~770 lines) that never reference `VulkanContext` itself. No function here exceeds 200 LOC by accurate brace-depth measurement.
**Proposed fix**: Extract the 5 `fill_*` methods into a new `mod telemetry;` and the data types into a new `mod types;` — both parallel to the existing `render_debug`/`resources` submodules. Together removes ~1160 of 2831 prod_loc, dropping the file well under 2000 without touching Vulkan lifecycle/drop-order code.

#### TD1-002: `storage_util.rs` — a 384-line/17-arm dispatcher and a 251-line declarative-call vec that should be a table
**Dimension**: 1 (File / Function / Module Complexity)
**Severity**: LOW
**File**: `crates/sdk/src/compatibility/storage_util.rs:1630-2013` (`adapt_storage_util_global_list`), `:420-670` (`papyrus_storage_util_declarations`)
**Effort**: small
**Finding**: `adapt_storage_util_global_list` (confirmed 384 lines by brace-depth) dispatches 17 `StorageUtilListCall` verbs, each carrying real distinct logic — a genuine "one function per verb" case, not a table candidate. `papyrus_storage_util_declarations` (251 lines) is the opposite shape: a flat `vec![...]` of ~35 near-identical declaration calls — a lookup-table candidate, and the file's own 2026-09-09 commit already performed exactly this conversion on a sibling 106-arm match elsewhere in the same file.
**Proposed fix**: Extract each `adapt_storage_util_global_list` arm body into a named helper (`list_op_add`, `list_op_sort`, …) behind a thin ~30-line dispatcher; replace `papyrus_storage_util_declarations`'s vec literal with a `const` tuple table fed through a small loop.

#### TD1-003: `crates/plugin/src/esm/records/mod.rs` mixes a re-export barrel with a 435-line parsing entry point
**Dimension**: 1 (File / Function / Module Complexity)
**Severity**: LOW
**File**: `crates/plugin/src/esm/records/mod.rs:185-619`
**Effort**: medium
**Finding**: The file legitimately barrels 21 `pub use` re-exports from already-split submodules, but also owns `parse_esm`, `character_rules_profile`, and `parse_esm_with_load_order` (confirmed 435 lines) — the latter mixing setup/per-GRUP-dispatch/assembly in one body via a 19-arm top-level GRUP match (below the 50-arm table trigger; each arm's FO4-gating differs too much for a table to help).
**Proposed fix**: Split the parsing entry point out to a sibling `records/parse.rs`, re-exported once from `mod.rs` to restore it as a pure barrel; independently extract the setup phase and heaviest GRUP arms (`LTEX`, `SCOL`, `PKIN`/`MOVS`/`MSWP`) into named helpers, matching how CELL/WRLD already delegate to `cell`'s walker.

#### TD2-001: `ssao.rs` hand-rolls an image barrier identical to an existing `descriptors.rs` helper
**Dimension**: 2 (Logic Duplication)
**Severity**: LOW
**File**: `crates/renderer/src/vulkan/ssao.rs:512-519`, `crates/renderer/src/vulkan/descriptors.rs` (`image_barrier_general_to_shader_read`, ~379-387)
**Effort**: trivial
**Finding**: `SsaoPipeline::dispatch`'s post-dispatch barrier is field-for-field identical to the existing helper; `ssao.rs` already imports sibling `descriptors::` helpers a few lines above — this one site was simply missed when the module was wired up. No divergent history on either side.
**Proposed fix**: Replace the manual construction with `super::descriptors::image_barrier_general_to_shader_read(ao_image)`.

#### TD2-002: UNDEFINED→SHADER_READ_ONLY_OPTIMAL init barrier duplicated verbatim in two files, no shared helper to catch it
**Dimension**: 2 (Logic Duplication)
**Severity**: LOW
**File**: `crates/renderer/src/vulkan/gbuffer.rs:330-338`, `crates/renderer/src/vulkan/frame_upscaler.rs:306-318`, `crates/renderer/src/vulkan/descriptors.rs`
**Effort**: small
**Finding**: Both build an identical one-shot discard-and-transition barrier with only the image list differing. `descriptors.rs` has the mirror-image `image_barrier_undef_to_general` (UNDEFINED→GENERAL) but no UNDEFINED→SHADER_READ_ONLY_OPTIMAL equivalent, so this shape had nowhere to funnel through. `caustic.rs::initialize_layouts` builds a close cousin over a layered subresource range, same root cause (no layered variant).
**Proposed fix**: Add `image_barrier_undef_to_shader_read(image) -> vk::ImageMemoryBarrier<'static>` to `descriptors.rs` mirroring `image_barrier_undef_to_general`; migrate `gbuffer.rs`/`frame_upscaler.rs` onto it; optionally add a `_layers` variant (following the existing `image_barrier_undef_to_transfer_dst`/`_layers` split) so `caustic.rs` can adopt it too.

#### TD2-003: `movs.rs`/`scol.rs` re-implement EDID/MODL/VMAD presence logic that `CommonNamedFields::from_subs_with_remap` already generalizes
**Dimension**: 2 (Logic Duplication)
**Severity**: LOW
**File**: `crates/plugin/src/esm/records/movs.rs:87-125`, `crates/plugin/src/esm/records/scol.rs:132-150`, `crates/plugin/src/esm/records/common.rs`
**Effort**: small
**Finding**: `common.rs` documents `from_subs_with_remap` as the intended single funnel, already followed at several tracked sites (`container.rs` #1045, `misc/world.rs` #2068, `misc/scene.rs` #2414). `movs.rs` hand-rolls its own EDID/MODL/VMAD loop instead. `scol.rs` *does* call the shared helper but its second SCOL-specific loop still re-adds a redundant `VMAD => has_script = true` arm, discarding the shared result in favor of a local shadow. No divergent bug-fix history between the copies.
**Proposed fix**: In `movs.rs`, replace the manual loop with `CommonNamedFields::from_subs_with_remap`; in `scol.rs`, drop the redundant arm and use the already-computed `common.has_script`.

#### TD4-001: `audit-esm/SKILL.md` backticks a test helper that no longer exists — the paragraph already names its replacement
**Dimension**: 4 (Audit-Finding Rot)
**Severity**: LOW
**File**: `.claude/commands/audit-esm/SKILL.md:276`
**Effort**: trivial
**Finding**: `record_parsers_with_embedded_form_ids_take_a_remap` no longer exists (`48acaea2` inverted the guard to `all_record_parsers()`); the same paragraph already narrates the rename two sentences later.
**Proposed fix**: Replace the backticked dead name with italics or the current `all_record_parsers()` name.

#### TD4-002: `audit-ecs/SKILL.md` cites a bare `boot.rs` split away over a month earlier; sibling skill already has the fix
**Dimension**: 4 (Audit-Finding Rot)
**Severity**: LOW
**File**: `.claude/commands/audit-ecs/SKILL.md:305`
**Effort**: trivial
**Finding**: `boot.rs` no longer exists (split into `byroredux/src/boot/` under #3855); `audit-fnv/SKILL.md:188` already carries the corrected pointer for the identical claim.
**Proposed fix**: Update to `` `byroredux/src/boot/schedule/post_update.rs`, post-#3855 split of the former `boot.rs` ``, mirroring `audit-fnv/SKILL.md:188`.

#### TD4-003: `audit-fnv/SKILL.md`'s own historical `boot.rs` mention is backticked, contrary to the gate's advisory convention
**Dimension**: 4 (Audit-Finding Rot)
**Severity**: LOW
**File**: `.claude/commands/audit-fnv/SKILL.md:188`
**Effort**: trivial
**Finding**: Prose is correct ("the former `boot.rs`") but still backticks a name that resolves nowhere in the tree; the gate's convention is italics for historical/dead names.
**Proposed fix**: Change to italics: "the former *boot.rs*".

#### TD6-005: FO4+ weapon-mod attach graph forced to `None` on the streaming-partial cell-load path
**Dimension**: 6 (Stub & Placeholder Implementations)
**Severity**: LOW (deliberate, tracked scope decision)
**File**: `byroredux/src/cell_loader/partial.rs:139-146`
**Effort**: small
**Finding**: The streaming-partial NIF import path always sets `attach_points`/`child_attach_connections` to `None`, while the synchronous path materializes both. Comment states this is a deliberate, tracked (#1594) scope decision ("cell-streamed REFRs are architecture/clutter, not modular weapons"), not an oversight.
**Proposed fix**: No action needed unless FO4 clutter REFRs are later found to carry weapon-mod attach graphs; otherwise keep the #1594 reference current.

#### TD6-006: `PerkRecord` doc comment is stale — claims CTDA/EPFD decode are still "follow-ups"
**Dimension**: 6 (Stub & Placeholder Implementations)
**Severity**: LOW (documentation-only)
**File**: `crates/plugin/src/esm/records/misc/magic.rs:220-226`
**Effort**: trivial
**Finding**: `parse_perk` already fully handles per-entry CTDA (`push_ctda`) and EPFD-by-function_type decode (types 1-5), contradicting the struct doc comment's claim these are deferred follow-ups. A regression test already exercises CTDA through `parse_perk`.
**Proposed fix**: Update the doc comment to reflect that CTDA/EPFD are implemented; name any specific unhandled `function_type` values explicitly if any remain (the `_ => PerkFunctionData::None` catch-all suggests some might).

#### TD8-001: Stale `#[cfg_attr(not(test), allow(dead_code))]` on `ActionState::was_released`
**Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
**Severity**: LOW
**File**: `byroredux/src/interaction.rs:692`
**Effort**: trivial
**Finding**: `was_released` now has a real production caller (`byroredux/src/extensions/systems.rs:754`, wired into the live schedule via `boot/schedule/late.rs:410`). The annotation is inert leftover from before that consumer landed.
**Proposed fix**: Delete the attribute; `cargo check -p byroredux` should stay warning-free.

#### TD8-002: Three stale `#[allow(dead_code)]` in `groundcover_translate.rs` — #4054 already gave them a production consumer
**Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
**Severity**: LOW
**File**: `byroredux/src/groundcover_translate.rs:82, 91, 172` (doc block at 61-72 also needs updating)
**Effort**: small
**Finding**: `SUPPRESSION_KEYWORDS`, `AFFINITY_KEYWORDS`, and `layer_affinity` all carry a dead-code annotation claiming no consumer exists; `#4054` wired `layer_affinity` into `cell_loader/terrain.rs`'s real (non-test) `CellSplatLayer` builder, which flows into `render/groundcover.rs`'s GPU upload. The sibling `layer_affinities` (plural) is still genuinely uncalled and should NOT be touched.
**Proposed fix**: Remove the three attributes; tighten the doc comment to say only the GPU `groundcover_scatter.comp` `affinity(splat)` dispatch is still pending, since the CPU-side consumer now exists.

#### TD9-001: Golden-frame pixel regression guard has zero effective coverage pending a human GPU regen
**Dimension**: 9 (Test Hygiene)
**Severity**: LOW
**File**: `byroredux/tests/golden_frames.rs:66-120`, `byroredux/tests/golden/cube_demo_60f.capture`
**Effort**: small (needs a Vulkan device + release build + one command; no code change)
**Finding**: The project's only pixel-level render regression guard cannot currently pass — the baseline PNG predates `--bench-mode renderer-static` (2026-08-11) and FSR3 becoming non-default (2026-07-22); 550+ renderer commits (272 touching shaders) have landed since the last real capture (`4376f7a6`, 2026-06-04). `#3849` (closed today via `f5127c1c`) converted the false-positive failure mode into an explicit staleness assertion, which is the correct mitigation for *that* problem but does not restore actual pixel-regression coverage.
**Proposed fix**: No source change needed — a one-time manual regeneration (`BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored cube_demo_golden_frame`) on a Vulkan-capable machine, committing the refreshed baseline + `.capture`.

## Checked Clean (documented, not counted as findings)

- **TD1-004** (Dim 1): `crates/nif/src/blocks/mod.rs`'s ~280-arm block-type dispatch match was checked against the "≥50 arms → lookup table" trigger and explicitly **not** recommended for conversion — it is a deliberately flat, audit-tracked correctness metric (CLAUDE.md, session memory) whose arm count itself is monitored across NIF-coverage audits; each arm is a trivial one-line mapping, not imperative logic worth tabling.
- **TD3-002** (Dim 3): `GpuCamera`/`GpuInstance`/`Vertex::SIZE` doc-comment figures cross-checked against their pinning tests — all current, no drift.
- **TD3-003** (Dim 3): Deleted render-time `Material::classify_pbr` — guarded by an existing self-enforcing test (`byroredux/src/workspace_hygiene_tests.rs`); no new stale framing found, including in `.md` files the test deliberately excludes.
- **TD9-002/003/004** (Dim 9): `#[ignore]` reason-string hygiene (188/188 carry a reason, 0 bare, 0 vague/issue-referencing), smoke-only `is_ok()`/`is_err()` assertions (all 53 are boundary-function tests where that's the complete contract), and the two manual perf-calibration benches in `draw_sort_key_tests.rs` — all confirmed clean, no defect.
- **Dimension 5** (Stale Markers): 22 raw hits across `crates`/`byroredux`/`tools`, 0 shader-side hits, **all 22 excluded** as documented false positives (ESM `XXXX` protocol tag, upstream-reference-implementation FIXME citations, the one accepted documented-unknown TBD in `items.rs`, retrospective/closed-issue prose). Third-party MIT attribution block in `triangle.frag` confirmed intact.
- **Dimension 7** (Magic Numbers): NIF version gates, Vulkan `MAX_*`/`MIN_*` constants, GPU `#[repr(C)]` size literals, frame/ray/cache budgets, and ESM sub-record size checks are all clean — named constants or protocol-defined magic throughout. The shader `#define` provenance gate's 2026-09-05 blind spot (TD7-02, `shaders/include/` unreachable) is confirmed **fixed** by `#3880`/`#3815`.
- **Dimension 8** (Dead Code): `cargo machete` reports zero unused deps; zero `#[deprecated]` items; zero `// removed:` breadcrumbs; all `[features]` flags across the workspace resolve to real multi-branch gates; `cargo check -p byroredux` is warning-clean under current annotations. ~15 other `allow(dead_code)` sites individually verified as correctly-justified (RAII guard fields, debug-assertions-gated, write-only-until-milestone, etc.) — not re-flagged.

## Deferred

- **TD6-001** (Havok constraint CInfo decode) is gated on PHYSAL's broader
  FO4+ `BhkSystemBinary` blocker per session memory (`physal_physics_layer.md`)
  for the FO4+ side, though it affects Oblivion→Skyrim content independently
  of that blocker — not fully deferred, but partially coupled to it.
- **TD6-003** (IMGS/IMAD HDR tonemap) — full implementation is large (DNAM +
  IMAD modifier graph + render consumer); realistically gated on a future
  cinematics/post-process milestone, not immediately actionable as a single fix.

No findings this cycle were gated on an in-progress milestone in a way that
blocks even a partial/doc fix — every MEDIUM has an actionable partial step
(documentation correction at minimum) available now.

---

**Next audit**: re-run the Phase 1 baseline commands and diff against this
report's Baseline Snapshot. Watch in particular: whether `context/mod.rs` and
`storage_util.rs` cross further growth or finally get split (TD1-001/002), and
whether the `GpuMaterial` self-enforcing test (TD3-001's proposed fix) lands
before the struct's fifth size change.

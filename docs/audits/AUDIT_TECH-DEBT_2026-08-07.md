# ByroRedux Tech-Debt Audit — 2026-08-07

Comprehensive 9-dimension sweep per `/audit-tech-debt`. Scope: code that
compiles, passes tests, and ships, but quietly raises the cost of every
future change — not correctness bugs (other audits own that).

Prior report: `docs/audits/AUDIT_TECH-DEBT_2026-08-03.md`.

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 35 |
| **Total** | **36** |

Zero CRITICAL/HIGH findings — consistent with this project's pattern of
frequent, tight audit cycles catching issues before they compound. The one
MEDIUM (TD3-208) is a stale GPU-struct-size doc comment sitting inside the
very file whose job is to be the lockstep-drift reference — promoted per the
severity table's explicit "stale `GpuMaterial`/`GpuInstance`/`GpuCamera` size
in a doc comment" trigger.

Delta vs. the Phase-1 baseline snapshot (`baseline.txt`, captured this
morning, same session): no dimension count moved — expected, since this is a
same-day continuation of one sweep, not a fresh audit cycle. See Baseline
Snapshot below for the numbers the *next* audit should diff against.

Two dimensions ran clean: **Dimension 5** (Stale Markers — 19 raw
TODO/FIXME/HACK/XXX hits, all resolve to protocol data or upstream-reference
documentation, zero actionable debt) confirms the marker hygiene this project
has maintained across several audit cycles. No dimension returned zero
findings *and* zero raw hits — Dimension 5 is "clean signal, present noise."

One cross-report duplicate is worth flagging up front: **TD6-001** (this
report, LOW) and **PAT-D6-01** (`AUDIT_LEGACY_COMPAT_2026-08-07.md`, MEDIUM)
describe the *same* gap — Skyrim+/FO4/FO76/Starfield `RACE` `DATA`
sub-record silently unparsed, `crates/plugin/src/esm/records/actor/mod.rs`
— reached independently through two different audit lenses (stub/placeholder
debt here, per-game translation-survey gap there). Treat as one underlying
issue when filing to GitHub; the legacy-compat audit's MEDIUM is the correct
severity to publish under (it reflects gameplay/compat impact), this report's
LOW reflects the narrower "is this a stub" framing. Do not file both.

## Baseline Snapshot (for the next audit's diff)

```
TODO/FIXME/HACK/XXX:    19   (Dim 5: 0 actionable — all protocol data / upstream-doc)
allow(dead_code):       48   (Dim 8: 46 justified, 1 existing #1761, 2 newly flagged)
unimplemented!/todo!(): 0    (Dim 6: confirmed still 0 on fresh re-run)
#[ignore] tests:        345  (Dim 9: ~119 of ~340 textual hits are real attributes;
                              rest are doc-comment/prose mentions of the string —
                              same undercount already tracked by open #2262/TD4-002)
files >2000 LOC:        9    (Dim 1: confirmed same 9-file set — see TD1-001..012)
```

## Top 10 Quick Wins

Trivial/small effort, immediate readability or drift-prevention payoff.

1. **TD8-002** — `byroredux-debug-ui`'s `Cargo.toml` declares the *entire
   Vulkan renderer crate* plus `egui-ash-renderer` and `anyhow` with zero
   source references. Delete all three lines; biggest single build-graph win
   in this sweep.
2. **TD8-005** — `byroredux-renderer` declares `byroredux-platform` and
   `winit` unused (window access goes through `raw-window-handle` only).
   Delete both — this is the workspace's heaviest crate, so every trimmed
   edge compounds.
3. **TD8-001** — `thiserror` declared but never invoked in `crates/bsa`,
   `crates/spt`, `crates/papyrus`, `crates/nif` (all four hand-roll their
   error types instead). Delete the dependency line from each.
4. **TD8-008** — `spawn_npc_entity`/`spawn_prebaked_npc_entity`
   (`byroredux/src/npc_spawn.rs`) are ~45 lines of verified-dead code left
   behind by the resumable-NPC-assembly rewrite (`9bf4c493`) — zero call
   sites anywhere. Delete.
5. **TD3-208** — `gpu_instance_layout_tests.rs`'s own field-order test still
   quotes the pre-#1657 300 B `GpuMaterial` size in its doc comment, inside
   the exact file that's supposed to be the drift-detection reference.
   Combine with the renderer audit's three sibling sites in the same
   cluster (`gpu_types.rs:84`, `constants.rs:168`) in one pass.
6. **TD9-001** — `an_unrecognized_pex_is_a_silent_miss`
   (`crates/scripting/tests/pex_recognize_e2e.rs:120-131`) discards its
   computed result with `let _ = ...` — the exact regression it's named to
   guard against (an overly broad recognizer accidentally matching) would
   pass silently. One-line `assert!` fix.
7. **TD9-002** — `gpu_instance_does_not_re_expand_with_per_material_fields`
   (`gpu_instance_layout_tests.rs:91-101`) builds a `Default::default()` and
   discards it — permanently green regardless of the struct. Also cites a
   stale 112 B size (current: 128 B). Add a real assertion or delete.
8. **TD7-002** — `skin.rs:226`'s SSE band check hardcodes `(100..130)` where
   the comment two lines above it *already names* `bsver::SKYRIM_SE`/
   `bsver::FALLOUT4` as the intended constants. Strongest instance of the
   three TD7 findings — the fix is already spelled out in-code.
9. **TD4-003** — Four saved audit reports (`AUDIT_TECH-DEBT_2026-07-16.md`,
   `AUDIT_TECH-DEBT_2026-08-03.md`, `AUDIT_LEGACY-COMPAT_2026-07-02.md`,
   `AUDIT_LEGACY-COMPAT_2026-07-16.md`) use a hyphen where every skill's own
   naming convention specifies an underscore, making them invisible to the
   glob the Phase-1 setup step actually runs. `git mv` to fix (note: this
   report itself currently follows the same hyphenated convention per
   explicit instruction — see naming note at file end).
10. **TD3-211 / TD3-212** — `ROADMAP.md`'s Known Issues list still shows
    REND-#1449/#1450 (closed 2 months ago) and the Oblivion TES-grounding row
    (closed 3 days ago, #2193) as open `[ ]` checkboxes. Flip both to `[x]`
    with closure notes in the file's existing style.

## Top 5 Medium Investments

File/function splits and duplication consolidations — plan before starting.

1. **TD1-001** — `draw_frame` (`crates/renderer/src/vulkan/context/draw.rs`,
   2052 LOC, cognitive complexity 91/25) has regrown past two prior partial
   extractions (#2197, #2255). Third attempt needs a phase-level split
   (`sync_and_acquire_frame`, `dispatch_tlas_blas_builds`,
   `upload_scene_buffers_for_frame`, `dispatch_cluster_light_culling`,
   `submit_and_present`) rather than another single-block extraction. No
   barrier/order changes — orchestration-only, render-pass-adjacent (see
   `feedback_speculative_vulkan_fixes.md`).
2. **TD1-012** — `parse_qust` (`crates/plugin/src/esm/records/misc/quest.rs`,
   646 LOC) has cognitive complexity **119/25** — the highest score measured
   anywhere in this audit, higher than `draw_frame`. Split by QUST data
   group (stages/objectives/aliases/fragments), mirroring the
   `esm/records/actor/` per-data-group precedent (#2055). QUST is an
   actively-growing subsystem this week — the next quest feature is likely
   to land inside this same function otherwise.
3. **TD1-002** — `VulkanContext::new` (1205 LOC, complexity 78/25) hasn't
   finished the `new_inner` delegation pattern six sibling subsystems
   (composite/volumetrics/svgf/ssao/bloom/taa) already use. Extract any
   still-inline subsystem block (FSR3, egui, presentation) into its own
   `new_inner`, called from here.
4. **TD1-006** — `load_references_budgeted`
   (`byroredux/src/cell_loader/references/mod.rs`, complexity 58/25) is the
   busiest touch point in cell loading (5 feature commits in the last week).
   Split per-REFR-type dispatch arms into named `dispatch_*` helpers over
   `&mut RefLoadAccum`/`&CellLoadCtx`; extract `spawn_synth_child` (488 LOC)
   to its own file.
5. **TD2-117** — ~28 sites across 10 `esm/records/misc/*.rs` files hand-roll
   the EDID/FULL(/MODL) sub-record bundle that
   `CommonNamedFields::from_subs` (`common.rs`) already exists to replace.
   TD2-109/#2068 fixed this only for `world.rs`; extend the same mechanical
   `let common = CommonNamedFields::from_subs(subs);` swap to `effects.rs`
   (8 sites), `character.rs`/`equipment.rs` (5 each), and the rest.

---

## Findings

### MEDIUM

#### TD3-208: `gpu_instance_layout_tests.rs`'s own `GpuMaterial` field-order test doc comments still quote the pre-#1657-era 300 B size — the pinning file misdescribes the very struct it pins
- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation & Comments)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:939` and `:990`
- **Status**: NEW
- **Description**: The authoritative pin for `GpuMaterial`'s current size is `gpu_material_size_is_348_bytes` (`crates/renderer/src/vulkan/material.rs:1271-1272`, `assert_eq!(std::mem::size_of::<GpuMaterial>(), 348)`) — the struct grew 300→348 B when twelve common supplemental texture-role indices landed (`1d94eb24`, 2026-07-27). But the doc comment on the sibling `gpu_material_glsl_field_order_matches_rust_struct` test — in the *same file* that carries the size pin — still describes a hypothetical metalness/roughness reorder as one that "preserves the 300 B size." Yesterday's `2aded28e` (TD3-204, #2308) fixed the same class of drift in `docs/engine/renderer.md` but did not touch this file.
- **Evidence**:
  ```
  939: /// `roughness`) that preserves the 300 B size — the shader would then
  990: /// keeps the 300 B size but corrupts every lit-surface read — see #1657 / SF-D8-01.",
  ```
  vs. `material.rs:1271-1272`: `assert_eq!(std::mem::size_of::<GpuMaterial>(), 348);`
- **Impact**: No runtime effect — the test is byte-agnostic (checks field *order*, not size) and remains correct/passing. Purely misleads a future reader of the exact file whose job is to be the lockstep-drift reference. Same file cluster as `AUDIT_RENDERER_2026-08-07.md`'s `REN-D3-2026-08-07-03`/`MAT-D7-2026-08-07-01`, which caught **three other** stale-size sites in the same cluster (`gpu_types.rs:84`, `gpu_instance_layout_tests.rs:97`, `constants.rs:168`) but not these two.
- **Related**: TD3-204/#2308 (CLOSED, `renderer.md` copy of this class), #2222 (CLOSED), same-day sibling renderer-audit findings (not yet published as GitHub issues as of this writing).
- **Suggested Fix**: Fix all five sites (this finding's two plus the renderer audit's three) in one pass — `gpu_types.rs:84`, `gpu_instance_layout_tests.rs:97` → 128; `constants.rs:168` → 348 B; this finding's `:939,990` → 348 B, or drop the literal number entirely and say "the current pinned size."
- **Age**: comment dates to 2026-06-18 (`78743032`), went stale 2026-07-27 (`1d94eb24`) — 11 days.
- **Effort**: trivial

---

### LOW

#### Dimension 1 — File / Function / Module Complexity

##### TD1-001: `draw_frame` re-grew past its #2255 partial fix — now 2052 LOC, cognitive complexity 91/25
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1179-3230`
- **Status**: Regression of #2255 (CLOSED)
- **Description**: The single largest function in the codebase. #2197/#2255 both landed real extractions, but the function absorbs new inline logic faster than it sheds old (TLAS-relocation, cluster-light-culling, fog/shadow-policy blocks). Actual GPU pass *recording* is already modularized out (`record_geometry_pass`, `record_post_passes`, `record_skinned_blas_refit` all called, not inlined) — remaining bulk is per-frame orchestration.
- **Related**: #2197, #2255 (CLOSED, insufficient), #2258/#2259 (sibling decompositions that held).
- **Suggested Fix**: See Top 5 Medium Investments #1.
- **Age**: regrown 2026-07-28 through 2026-08-04, after #2255's fix.
- **Effort**: large

##### TD1-002: `VulkanContext::new` — 1205 LOC, cognitive complexity 78/25
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:1672-2876`
- **Status**: NEW
- **Description**: Full Vulkan init chain (CLAUDE.md invariant #6) is architecturally expected to be a long sequence, but still inlines per-subsystem setup (SVGF, SSAO, composite, bloom, volumetrics, water, TAA, presentation, egui, texture/mesh registry, acceleration structures) rather than delegating, unlike 6+ sibling subsystems that already have their own `new_inner`.
- **Related**: TD1-003 (same file's `Drop::drop`), TD1-008/009 (subsystems already following `new_inner`).
- **Suggested Fix**: See Top 5 Medium Investments #3.
- **Age**: grown via egui-pass/FSR3/presentation wiring through `727b0e29`, 2026-08-05.
- **Effort**: large

##### TD1-003: `impl Drop for VulkanContext` — 343 LOC, cognitive complexity 36/25
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:3363-3705`
- **Status**: NEW
- **Description**: Reverse-order teardown mirroring `new()`. Lower priority than TD1-002 — a flat, explicitly-ordered destroy sequence is the *correct* shape for Vulkan teardown; over-abstracting `Drop` risks hiding ordering bugs.
- **Suggested Fix**: Do not extract opaque helpers that hide ordering. If split, only pair with existing `new_inner`/`destroy` pairs. Low priority, opportunistic alongside TD1-002.
- **Effort**: medium (if attempted), otherwise skip

##### TD1-004: `save_io.rs` — 2860 LOC, but production code is a healthy ~970 LOC; crossed 2000 via a ~1890-LOC inline `mod tests`
- **Severity**: LOW
- **Location**: `byroredux/src/save_io.rs:1-970` (production), `:971-2860` (tests)
- **Status**: NEW
- **Description**: No production function exceeds complexity 25. File crossed threshold via test bulk — a 209-LOC completeness-guard test (#2295) and an 81-LOC round-trip test.
- **Related**: Same pattern as TD1-009 (`material.rs`), partially TD1-005.
- **Suggested Fix**: Extract `mod tests` into sibling `*_tests.rs` files by topic — repo already has this convention (`cell_loader/{...}_tests.rs`, `scene_buffer/{...}_tests.rs`). Zero behavior risk.
- **Effort**: small

##### TD1-005: `crates/scripting/src/scene.rs` — newly crossed 2000 LOC via diffuse quest/scene-lifecycle growth, not a monolithic dump
- **Severity**: LOW
- **Location**: `crates/scripting/src/scene.rs:1-1335` (production), `:1336-2375` (tests)
- **Status**: NEW
- **Description**: Growth is 5 commits over 8 days (SCEN playback, PACK actions, save/load registration, #2295 cross-ref, today's quest-alias/lifecycle + observability work). None of its functions exceed complexity 25. File now combines 4 responsibilities (scene registry/playback, quest-alias injection, package-action execution, actor-binding resolution) that arrived as separate features.
- **Related**: TD1-004 (same inline-test-bulk pattern), #2295.
- **Suggested Fix**: (1) extract `mod tests` to per-topic siblings; (2) split production code along the commit-history-revealed boundaries: `scene_playback.rs`, `quest_alias.rs`, `package_action.rs`.
- **Age**: file is 8 days old (`6df3bad8`, 2026-07-31); crossed 2000 LOC today.
- **Effort**: medium

##### TD1-006: `byroredux/src/cell_loader/references/mod.rs` — newly crossed 2000 LOC; `load_references_budgeted` (723 LOC, cc 58/25) and `spawn_synth_child` (488 LOC, cc 31/25)
- **Severity**: LOW
- **Location**: `byroredux/src/cell_loader/references/mod.rs:200-922`, `:1099-1586`
- **Status**: NEW
- **Description**: Resumable per-cell REFR loading job driver; busiest touch point in cell loading this week (5 feature commits). Also flags a `clippy::type_complexity` hit on `current_ref_synth`'s type at `:70`.
- **Related**: TD1-007 (same subsystem/week), #2277.
- **Suggested Fix**: See Top 5 Medium Investments #4.
- **Age**: crossed threshold this week (2026-08-04 through 2026-08-07).
- **Effort**: medium

##### TD1-007: `byroredux/src/cell_loader/spawn.rs` — newly crossed 2000 LOC; `spawn_mesh_instance` (546 LOC) and `spawn_placed_instances` (240 LOC) are genuine production bloat, not test code
- **Severity**: LOW
- **Location**: `byroredux/src/cell_loader/spawn.rs:1205-1750`, `:385-624`
- **Status**: NEW
- **Description**: Neither function appears in the clippy cognitive-complexity report — bulk is straight-line per-attribute vertex assembly (positions/normals/UVs/tangents/skin-weights with fallback defaults), not deep branching.
- **Related**: TD1-006 (same subsystem, same week's commits).
- **Suggested Fix**: Extract `build_vertex_buffer(mesh: &ImportedMesh) -> Vec<Vertex>` covering the per-attribute fallback logic; check against the loose-NIF loader's equivalent for Dimension-2 duplication before splitting.
- **Age**: crossed threshold today (`8ee151e0`/`716b7ee9`, 2026-08-07).
- **Effort**: medium

##### TD1-008: `crates/renderer/src/vulkan/volumetrics.rs` crossed 2000 LOC — `new_inner` is a genuine 555-LOC production constructor
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:544-1098`
- **Status**: Existing: #2256 (OPEN) — confirmed still accurate, 555 LOC (off-by-one from #2256's "556" figure).
- **Description**: 87% of the 2165-LOC file is production code; `new_inner` is sequential Vulkan resource setup, not deep branching.
- **Suggested Fix**: (Restating #2256.) Split by resource category — froxel-grid allocation, descriptor layout/pool, pipeline creation, shadow-policy setup.
- **Effort**: medium

##### TD1-009: `crates/renderer/src/vulkan/material.rs` crossed 2000 LOC — confirmed test-only growth, no oversized production function
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/material.rs:1-1246` (production), `:1247-2015` (tests)
- **Status**: Existing: #2257 (OPEN) — confirmed still accurate.
- **Suggested Fix**: (Restating #2257.) Extract `mod tests` to sibling `*_tests.rs` files mirroring `scene_buffer/{...}_tests.rs`.
- **Effort**: small

##### TD1-010: `byroredux/src/asset_provider/tests.rs` — pure test file (2011 LOC), already has clear per-topic section markers ready to become file boundaries
- **Severity**: LOW
- **Location**: `byroredux/src/asset_provider/tests.rs` (whole file)
- **Status**: NEW
- **Description**: 8 explicit topic-divider comments already split the file logically (M35 sibling archive, BGSM merge #493 — the largest at ~1200 lines, Starfield `.mat`, etc).
- **Related**: #2311 (the sibling `crates/nif/src/import/tests/` split — same pattern, proven).
- **Suggested Fix**: Convert to a `tests/` directory mirroring the `import/tests/` precedent: `mod.rs`, `archive_siblings.rs`, `material_path.rs`, `bgsm_merge.rs`, `starfield_mat.rs`. Zero logic change.
- **Effort**: small

##### TD1-011: `byroredux/src/render/lights.rs::collect_lights` regressed back over 200 LOC — closed #2310 has silently regrown by 8 lines
- **Severity**: LOW
- **Location**: `byroredux/src/render/lights.rs:106-313`
- **Status**: Regression of #2310 (CLOSED)
- **Description**: Now 208 LOC. Regrew via #2250/#2205 light-kind-dispatch commits after #2310 trimmed it under 200 to close.
- **Impact**: Trivial — 8 lines over a soft convention threshold; demonstrates the 200-LOC convention has no regression guard.
- **Suggested Fix**: Not worth a dedicated split for 8 lines — let it ride until the next real edit, then trim the newest addition into a helper. Recorded so the next sweep doesn't treat this as fresh.
- **Effort**: trivial

##### TD1-012: Repo-wide >200-LOC function sweep outside the 9 named files — mostly inherent-complexity parsers/setup, two standouts
- **Severity**: LOW
- **Location**: various (104-entry scan; two actionable standouts below)
- **Status**: NEW (consolidated finding)
- **Description**: 104 functions >200 LOC repo-wide. Majority are accepted shapes (ESM/NIF field-proportional parsers, Vulkan `new_inner` constructors, scene/world bootstrap sequences) — re-flagging these repeats a pattern this repo's audit-hygiene memory warns against. Two genuine outliers:
  1. `crates/plugin/src/esm/records/misc/quest.rs::parse_qust` (646 LOC) — cognitive complexity **119/25**, highest of any function measured in this audit.
  2. `byroredux/src/asset_provider/material.rs::merge_external_material` (678 LOC) — complexity 35/25, the single NIFAL boundary function every per-game material audit finding routes through; size/complexity concern only, correctness is `/audit-nifal`'s territory.
- **Suggested Fix**: File a follow-up for `parse_qust` specifically (see Top 5 Medium Investments #2), mirroring the `esm/records/actor/` per-data-group precedent (#2055). Leave the accepted-shape bucket alone.
- **Effort**: medium (for `parse_qust`); no action on the rest

---

#### Dimension 2 — Logic Duplication

##### TD2-116: Undef→transfer-dst→shader-read barrier pair hand-rolled 3x instead of calling the existing `descriptors.rs` helpers
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/exposure.rs:150-199`, `ssao.rs:413-478`, `placeholder.rs:279-293` vs. `descriptors.rs:263-312`
- **Status**: NEW
- **Description**: `descriptors.rs` already exposes parameterized `image_barrier_undef_to_transfer_dst_layers`/`image_barrier_transfer_dst_to_shader_read_layers`. Three sites hand-roll the identical `vk::ImageMemoryBarrier` pair instead. `exposure.rs` (new file, 2026-07-22) additionally reintroduced a stale `TOP_OF_PIPE` stage-mask idiom the rest of the family already migrated off of, despite being brand-new — concrete evidence of the divergence risk this duplication shape creates.
- **Related**: TD2-112/113/114 (prior consolidations in this file family), TD2-NEW-01 (#2200, same category, already fixed).
- **Suggested Fix**: Replace hand-rolled pairs with the existing helpers in all three files; switch `exposure.rs`'s stage mask to `PipelineStageFlags::NONE` while touching it.
- **Age**: `exposure.rs` new (2026-07-22); `ssao.rs`/`placeholder.rs` copies older.
- **Effort**: trivial (~15 min)

##### TD2-117: EDID/FULL(/MODL) sub-record bundle hand-rolled ~28x across 10 `esm/records/misc/*.rs` files instead of calling `CommonNamedFields::from_subs`
- **Severity**: LOW
- **Location**: `character.rs` (5 sites), `dialogue.rs` (2), `effects.rs` (8), `equipment.rs` (5), `imagespace.rs` (1), `magic.rs` (4), `pack.rs` (1), `quest.rs` (1), `scene.rs` (1), `water.rs` (1) vs. `common.rs:249-296`
- **Status**: NEW (not a regression of TD2-109/#2068, which was scoped only to `world.rs`)
- **Description**: `CommonNamedFields::from_subs` already exists and is safe to call unconditionally (unmatched sub-record types are ignored). All ~24 FULL sites verified byte-identical (`read_lstring_or_zstring`) — mechanical copy-paste, not yet-diverged logic. Per the codebase's own history (FULL localization changed twice — #348, #989 — and both had to be hunted across every hand-rolled site before `CommonNamedFields` existed), a third such fix would again require manual propagation across ~24 sites.
- **Related**: TD2-109/#2068 (CLOSED, `world.rs` only), TD2-110/111 (#2069/#2070, same category different sub-record family, fixed).
- **Suggested Fix**: See Top 5 Medium Investments #5.
- **Effort**: small (~2-3 hrs across all 10 files, mechanical, zero current divergence to reconcile)

---

#### Dimension 3 — Stale Documentation & Comments

##### TD3-209: feature-matrix.md's three "Havok `.hkx` loader ✗" rows are stale — `crates/hkx` shipped 6 days ago and is wired into a real animation catalog
- **Severity**: LOW
- **Location**: `docs/feature-matrix.md:83,117,197` (and `:183`)
- **Status**: NEW
- **Description**: `crates/hkx` (`02c24e4f`, 2026-08-01) is a real, tested reader wired into the animation asset provider to install the MQ101 cart-idle catalog from real game data — deliberately scoped as a vertical slice, which is why "Partial" (the convention this file already uses elsewhere) fits better than a blanket ✗.
- **Related**: 5th consecutive cycle of the "feature docs lag feature code" pattern (see Recurring Pattern note below).
- **Suggested Fix**: Change the rows to `~ Partial` with a one-line scope note.
- **Age**: 6 days.
- **Effort**: trivial

##### TD3-210: feature-matrix.md has no Quests/M43 section at all, despite two sessions of substantial quest-lifecycle + alias-runtime work; the file's own "as of" date-stamp is 6+ weeks stale
- **Severity**: LOW
- **Location**: `docs/feature-matrix.md` (whole file — no Quests section); staleness marker at `:189` ("as of 2026-06-25")
- **Status**: NEW
- **Description**: ROADMAP.md's M43 row describes substantial, recently-landed runtime coverage (version-aware QUST lifecycle, Papyrus quest effects, alias fill/conditions/reservations, faction/inventory injections — `a844c26b`, `0775df28`) with zero corresponding coverage in the file whose stated remit is exactly "what do you see at runtime."
- **Related**: Same recurring pattern as TD3-209 (5th consecutive cycle).
- **Suggested Fix**: Add a `## Quests (M43)` section mirroring the Scripting (M47) table's shape; bump the stale date stamp.
- **Age**: quest work landed same-day; date stamp itself ~6 weeks stale.
- **Effort**: small

##### TD3-211: ROADMAP.md's Known Issues section still shows REND-#1449/#1450 as open (`[ ]`) — both issues closed over two months ago
- **Severity**: LOW
- **Location**: `ROADMAP.md:938-939`
- **Status**: NEW
- **Description**: Both closed same-day-filed, 2026-06-04 (`gh issue view` confirmed `CLOSED`), neither reflected in ROADMAP's checkbox/prose.
- **Suggested Fix**: Flip both checkboxes to `[x]`, prepend "**Closed 2026-06-04** —" per the file's own convention.
- **Age**: 2 months.
- **Effort**: trivial

##### TD3-212: ROADMAP.md's TES-grounding row still frames Oblivion `is_grounded` as unresolved; #2193 closed 3 days ago with a landed fix
- **Severity**: LOW
- **Location**: `ROADMAP.md:895`
- **Status**: NEW
- **Description**: Line 895 reads as an open HIGH investigation ("root cause not yet isolated"); #2193 is CLOSED (2026-08-04) with a landed fix (`195fbb28`) and verified grounded frame-0 through 120 frames on the exact repro cell.
- **Related**: #2013, #1832 (both closed, correctly marked elsewhere in the same file).
- **Suggested Fix**: Flip checkbox to `[x]`, strike "root cause not yet isolated," append a closure note matching the file's style.
- **Age**: 3 days.
- **Effort**: trivial

**Recurring Pattern (Dim 3):** 5th consecutive cycle (07-16, 07-25, 08-02,
08-03, today) where the dominant signal is `feature-matrix.md` lagging
shipped code rather than in-code comment drift. `ROADMAP.md` stays current
(verified this cycle); `feature-matrix.md` — the "second tier" doc — does
not get a matching per-session pass. Worth a process fix (fold a
feature-matrix diff-check into whatever already keeps ROADMAP.md current)
rather than a sixth data point next cycle.

---

#### Dimension 4 — Audit-Finding Rot

##### TD4-001: Crate-count roster regressed to stale again — two skill files still say 22/23 crates, one day after `crates/mod-runtime` bumped the live count to 24
- **Severity**: LOW
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md:21`, `.claude/commands/audit-scripting/SKILL.md:35`
- **Status**: NEW (regression of the pattern behind closed #2261)
- **Description**: `_audit-common.md` itself is correct (24, includes `mod-runtime`, added `9f619355` 2026-08-06); two *other* skill files still quote 23/22.
- **Related**: Regression of #2261 (same underlying pattern, second crate addition in a row to cause it).
- **Suggested Fix**: Bump both to 24; consider dropping the literal number from pointer sentences entirely.
- **Age**: 1 day.
- **Effort**: trivial

##### TD4-002: `_audit-common.md` disagrees with itself on the shader count — "21 GLSL sources" vs. "All 19 shaders"
- **Severity**: LOW
- **Location**: `.claude/commands/_audit-common.md:63` vs. `:104`
- **Status**: NEW
- **Description**: Same file, three lines apart in different tables, two different numbers for the same shader set. `ls crates/renderer/shaders/*.{vert,frag,comp}` confirms 21, matching line 63's explicit list; line 104 undercounts by 2 and doesn't correspond to any count `shader-pipeline.md` itself asserts.
- **Related**: Same self-inconsistency class as closed #2194 (table omission vs. this being a table undercounted total).
- **Suggested Fix**: Change line 104 to "All current shaders" (no number) or update to 21 with a re-count-don't-trust-prose note.
- **Effort**: trivial

##### TD4-003: Two audit report filenames use a hyphen instead of the skill-mandated underscore, making them invisible to the `AUDIT_TECH_DEBT_*.md`/`AUDIT_LEGACY_COMPAT_*.md` glob
- **Severity**: LOW
- **Location**: `docs/audits/AUDIT_TECH-DEBT_2026-07-16.md`, `AUDIT_TECH-DEBT_2026-08-03.md`, `AUDIT_LEGACY-COMPAT_2026-07-02.md`, `AUDIT_LEGACY-COMPAT_2026-07-16.md`
- **Status**: NEW
- **Description**: Both skills' own naming conventions specify underscores; four saved reports use hyphens instead, silently skipped by the exact glob the Phase-1 setup step runs.
- **Impact**: Each report's own "Prior report:" prose pointer correctly names its true predecessor regardless of filename, so the narrative chain hasn't broken — the exposure is to programmatic discovery only.
- **Suggested Fix**: `git mv` the four files to underscore-correct names.
- **Age**: oldest instance 36 days, newest 4 days — a recurring slip, not a one-off. (Note: this very report was requested under the hyphenated filename by explicit user/coordinator instruction — see closing note.)
- **Effort**: trivial

---

#### Dimension 5 — Stale Markers (TODO / FIXME / HACK / XXX)

**No findings.** 19 raw marker-lines found (`crates/`, `byroredux/`); 0 in
`crates/renderer/shaders/`. Of the 19: 16 are the `XXXX` ESM extended-size
protocol tag (explicit exclusion, not a debt marker), 2 are `// FIXME`
documenting an *upstream reference implementation's own* FIXME (`nifly`,
`bgem.rs`/`bs_geometry.rs` — not our debt), and 1 is a past-tense comment
correctly describing a closed issue (`scene.rs:1090`, verified both
referenced issues CLOSED and the fix in place). One location (`wrld.rs:175`
class) is already tracked by open **#2263** (the audit skill's own
exclusion-list text being incomplete) — not re-filed. Three prior closures
(#1111, #1110, #1627) spot-checked, all still hold. `triangle.frag`'s
third-party MIT attribution block verified present and intact.

---

#### Dimension 6 — Stub & Placeholder Implementations

##### TD6-001: Skyrim+ RACE `DATA` (height/weight/skill bonuses) silently unparsed
- **Severity**: LOW
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs:1017-1030` (the `b"DATA"` match arm), field docs at `:301-320`
- **Status**: NEW
- **Description**: The `RACE` `DATA` parse arm is gated `Oblivion | Fallout3NV` — Skyrim+'s 128/164-byte layout falls to `_ => {}`, leaving `skill_bonuses`/`height_*`/`weight_*`/`race_flags` at defaults for every Skyrim+ `RaceRecord`. The gate is intentional (per #1629, which stopped the TES4 36-byte layout from mis-decoding Skyrim's bytes into garbage) but no follow-up ever added the TES5 layout. Zero downstream consumer reads these fields for *any* game today, so current impact is nil.
- **Impact**: None today (no consumer exists). Becomes a real, silent divergent-by-game correctness gap the moment RACE height/weight/skill-bonus data is wired into NPC mesh scaling or CHARAL skill derivation for Skyrim+.
- **Related**: #1629 (CLOSED, introduced the current gate), #2093 (CLOSED, related Skyrim RACE gap). **Cross-referenced with `AUDIT_LEGACY_COMPAT_2026-08-07.md`'s PAT-D6-01** (MEDIUM, same file/gap, per-game translation-survey lens) — same underlying issue reached independently through two audit angles; file once, under PAT-D6-01's severity.
- **Suggested Fix**: File a follow-up for a TES5-shaped `DATA` decode arm (128-byte Skyrim / 164-byte SE+), gated the same way the TES4 arm is. Low priority while no consumer reads the fields; natural home is whichever milestone first wires RACE height/weight into NPC spawn (CHARAL Skyrim ruleset).
- **Effort**: small (≤2h — field layout documented, gating pattern exists to copy)
- **Age**: gate introduced by #1629; the TES5 gap itself predates that and was never separately tracked.

(41 other stub/placeholder-phrase hits triaged and excluded this cycle — test
fixtures, unrelated senses of "not yet," working-as-designed deliberate
choices, historical documentation of already-fixed bugs, and known/tracked
architecture limits with their own design docs. See dimension method notes;
not reproduced here to avoid padding.)

---

#### Dimension 7 — Magic Numbers & Hardcoded Constants

##### TD7-001: `NiGeomMorpherController`/`NiMorphData` legacy-field gates use bare `bsver` literals `9`/`10` instead of a named constant
- **Severity**: LOW
- **Location**: `crates/nif/src/blocks/controller/morph.rs:103` (`bsver > 9`), `:207` (`bsver < 10`)
- **Status**: NEW
- **Description**: `version.rs`'s `bsver` module explicitly documents the project convention of using named constants over bare decimal literals. The numerically-adjacent `bsver::RIGID_BODY_EXTRA_FLOATS = 9` is semantically unrelated (its own doc comment warns against misattribution) — needs new constants, not a repoint.
- **Related**: Same drift class as #2343 (OPEN), #2281 (CLOSED), #1336/#1319/#1630/#1042 — none cover `morph.rs`.
- **Suggested Fix**: Add `bsver::MORPHER_TRAILING_INTS: u32 = 9` / `bsver::MORPH_DATA_LEGACY_WEIGHT: u32 = 10`, point both call sites at them.
- **Age**: `:103` 2 weeks, `:207` ~3 months.
- **Effort**: trivial

##### TD7-002: `NiSkinPartition`'s SSE band check uses bare `100`/`130` where `bsver::SKYRIM_SE`/`bsver::FALLOUT4` already exist with matching semantics
- **Severity**: LOW
- **Location**: `crates/nif/src/blocks/skin.rs:226`
- **Status**: NEW
- **Description**: `(100..130).contains(&bsver)` hardcodes exactly the `[SKYRIM_SE, FALLOUT4)` band that already has named constants — the surrounding comment *already names the constant in prose* (describing the historical #126/NIF-206 bug) without the code using it.
- **Related**: Same class as TD7-001/003, #2343 (OPEN), #2281 (CLOSED).
- **Suggested Fix**: `let is_sse = (bsver::SKYRIM_SE..bsver::FALLOUT4).contains(&bsver);` — one-line, no behavior change.
- **Age**: ~3.5 months.
- **Effort**: trivial

##### TD7-003: `NiControllerSequence`'s FO3/FNV anim-notes branch uses bare `24..=28` immediately below a sibling branch using the named `ANIM_NOTES_THRESHOLD`
- **Severity**: LOW
- **Location**: `crates/nif/src/blocks/controller/sequence.rs:305`
- **Status**: NEW
- **Description**: Two branches on the same `bsver` variable three lines apart — the first correctly uses `bsver::ANIM_NOTES_THRESHOLD` (28), the very next `else if` re-hardcodes both bounds. Lower bound (24) has no existing named constant with matching semantics (`bsver::FO3_PARALLAX = 24` is numerically identical but semantically unrelated).
- **Related**: `#2281` (CLOSED, same file, different line — a sibling site the original fix didn't reach), TD7-001, TD7-002.
- **Suggested Fix**: Add `bsver::FO3_ANIM_NOTES_LOWER: u32 = 24`, rewrite as a range using both named constants.
- **Age**: ~3.5 months.
- **Effort**: trivial

**Rediscovered (not re-filed):** `shader.rs:1026`'s bare `(130..=139)` FO4-DLC
band check — already tracked by open **#2343**.

**Recurring Pattern (Dim 7):** 6th identifiable wave of "bare `bsver` literal
bypasses the named constant module" (#1042 → #1319 → #1336 → #1630 → #2281 →
#2343 (open) → today). Two of this cycle's three new sites sit in files with
a correctly-named sibling comparison 2-3 lines away — suggesting a
habit/review-checklist gap rather than unfamiliarity. Worth a pre-commit
grep/lint rather than relying on audit cycles to keep catching it.

Everything else in scope — Vulkan `MAX_*`/`MIN_*` centralization, shader
`#define` provenance (zero bypasses of the generated-header pipeline), GPU
`#[repr(C)]` size literals in code (all route through `size_of::<Gpu*>()`),
budget centralization, ESM sub-record sizes — verified clean; see dimension
notes for the specific checks performed.

---

#### Dimension 8 — Dead Code & Backwards-Compat Cruft

##### TD8-001: `thiserror` declared as a direct dependency but never referenced in 4 crates
- **Severity**: LOW
- **Location**: `crates/bsa/Cargo.toml:8`, `crates/spt/Cargo.toml:18`, `crates/papyrus/Cargo.toml:8`, `crates/nif/Cargo.toml:9`
- **Status**: NEW
- **Description**: `cargo machete` flags all four; verified by hand — none use `thiserror::`/`#[derive(Error)]` anywhere. Each hand-rolls its error type instead.
- **Suggested Fix**: Remove the dependency line from each; confirm with `cargo check`.
- **Effort**: trivial

##### TD8-002: `byroredux-debug-ui` declares 3 dependencies with zero source references
- **Severity**: LOW
- **Location**: `crates/debug-ui/Cargo.toml:8` (`byroredux-renderer`), `:11` (`egui-ash-renderer`), `:13` (`anyhow`)
- **Status**: NEW
- **Description**: Pulls in the *entire Vulkan renderer crate* (and its transitive `ash`/`gpu-allocator`/`rspirv`/`fsr3-sys`) purely on paper — real build-graph bloat, and contradicts the crate's own module doc ("the renderer stays a pure-GPU layer").
- **Suggested Fix**: Delete all three lines.
- **Age**: ~2.5 months, survived a later unrelated cleanup pass on the same file.
- **Effort**: trivial

##### TD8-003: `byroredux-ui` declares `byroredux-core` and `ruffle_render` as unused dependencies
- **Severity**: LOW
- **Location**: `crates/ui/Cargo.toml`
- **Status**: NEW
- **Description**: Both confirmed zero references. A third flagged dep, `image`, is a softer call — reachable transitively via `ruffle_render_wgpu`'s inherent methods, so the explicit pin may be intentional; not recommended for deletion without a decision.
- **Suggested Fix**: Remove `byroredux-core` and `ruffle_render` (not `ruffle_render_wgpu`); leave `image` pending a version-pin decision.
- **Effort**: trivial

##### TD8-004: `byroredux-platform` declares `byroredux-core` as a dependency with zero references
- **Severity**: LOW
- **Location**: `crates/platform/Cargo.toml`
- **Status**: NEW
- **Description**: The crate's own module doc describes itself as a deliberately small, dependency-light `winit` wrapper — consistent with the empty grep.
- **Suggested Fix**: Remove the line.
- **Effort**: trivial

##### TD8-005: `byroredux-renderer` declares `byroredux-platform` and `winit` with zero references
- **Severity**: LOW
- **Location**: `crates/renderer/Cargo.toml`
- **Status**: NEW
- **Description**: The renderer talks to the window purely through `raw-window-handle` trait objects, never needing concrete `winit`/`platform` types.
- **Suggested Fix**: Remove both lines; full workspace build afterward given this crate's many downstream consumers.
- **Effort**: trivial

##### TD8-006: `crates/debug-ui/src/lib.rs`'s `pub use egui; pub use egui_winit;` have zero downstream consumers
- **Severity**: LOW
- **Location**: `crates/debug-ui/src/lib.rs` (final two lines)
- **Status**: NEW
- **Description**: Stated justification ("so the binary doesn't have to add a direct dep") never materialized — `byroredux/Cargo.toml` never added `egui`/`egui-winit` directly, and `main.rs` never routes through this re-export. `egui_pass.rs` declares its own direct `egui` dep instead. Same class as closed #1324 but a different, still-live pair (that fix targeted `pub` functions, not this type re-export).
- **Suggested Fix**: Delete both lines; re-add with an actual call site if ever needed.
- **Age**: ~2.5 months.
- **Effort**: trivial

##### TD8-007: `RawDependency::name` is deserialized from `plugin.toml` and immediately discarded
- **Severity**: LOW
- **Location**: `crates/plugin/src/manifest.rs:73` (field), `:44-48` (conversion)
- **Status**: NEW
- **Description**: The manifest format's own doc example shows a `name` field on `[[dependencies]]`, and it genuinely deserializes — but conversion into `PluginManifest.dependencies` keeps only the UUID, dropping the human-readable name. Worse UX than deleting the field outright: dependency-resolution failures can currently only report a bare UUID.
- **Suggested Fix**: Either delete the field (cheapest) or thread it through to enable named error messages in `resolver.rs` (higher value, given the data is already parsed for free).
- **Effort**: small (≤2h if wiring through; trivial if deleting)

##### TD8-008: `spawn_npc_entity` / `spawn_prebaked_npc_entity` are unreachable dead code after the resumable-job rewrite
- **Severity**: LOW
- **Location**: `byroredux/src/npc_spawn.rs:716`, `:815`
- **Status**: NEW
- **Description**: Both `#[allow(dead_code)]`, documented as "compatibility entry points," zero call sites anywhere (production or test) after `9bf4c493` (2026-07-27) replaced their ~1045-line body with the thin `NpcSpawnJob::runtime(...).advance(...)` wrapper this pair now duplicates uselessly. ~45 lines of unreachable logic that will silently bit-rot if `NpcSpawnJob`'s signature changes.
- **Suggested Fix**: Delete both unless a specific near-term caller (e.g. sync debug/test harness) is already planned.
- **Age**: ~1 week — a fresh regression from an otherwise legitimate refactor, not old rot.
- **Effort**: trivial

**Existing (not re-filed):** #1761 — `Dx10Chunk::start_mip`'s `#[allow(dead_code)]` is now stale (confirmed live-read today); `end_mip` correctly remains muted (M40 streaming). Issue is accurate; not re-filed.

Verified clean: RAII guard fields (documented Drop-only), `cfg(debug_assertions)`/`cfg(feature = "debug-server")`-gated fields, forward-looking bit catalogs with test coverage, `crates/plugin/src/legacy/` module-level allow (settled per #1322), `_unused`-named locals consuming documented protocol padding (not the CLAUDE.md refactor-survivor anti-pattern), `_`-prefixed trait-impl parameters (~40 spot-checked, all fixed-signature interface requirements), zero `#[deprecated]`, zero `// removed:` breadcrumbs, all Cargo feature flags have a documented real enabling path. Prior #1049 batch closures re-verified, all held.

---

#### Dimension 9 — Test Hygiene

##### TD9-001: `an_unrecognized_pex_is_a_silent_miss` discards its own result, asserting nothing
- **Severity**: LOW
- **Location**: `crates/scripting/tests/pex_recognize_e2e.rs:120-131`
- **Status**: NEW
- **Description**: The test's own comment states the contract it exists to verify (a vanilla script should translate to `None`), then computes `translate_pex(...)` and discards it with `let _ = ...`. If `ObjectReference.pex` ever became accidentally recognized by an overly broad future recognizer — the exact regression the test's name says it guards against — it would still pass silently.
- **Suggested Fix**: `assert!(got.is_none(), ...)` on the actual return value.
- **Age**: ~5 weeks.
- **Effort**: trivial

##### TD9-002: `gpu_instance_does_not_re_expand_with_per_material_fields` is a no-op test (and cites a stale byte size)
- **Severity**: LOW
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:91-101`
- **Status**: NEW
- **Description**: Builds `GpuInstance::default()` and discards it — since `GpuInstance` already implements `Default`, this cannot fail under any circumstance; permanently green. Comment also cites a stale "112 B" (current: 128 B since #2219, same commit that last touched this test).
- **Suggested Fix**: Either delete (duplicates the working sibling size guard, `gpu_instance_is_128_bytes_std430_compatible`) or add a real inline assertion; fix "112 B" → "128 B" in whichever survives.
- **Age**: last touched `c4cb26146`, 2026-08-03.
- **Effort**: trivial

Verified clean: zero `#[ignore]`d tests guard a fix from a closed CRITICAL/HIGH issue (all 27 issue-referencing `#[ignore]` sites cite the issue as provenance for a real-data count/threshold, not a dormant fix-guard, and are all *also* real-game-data-gated regardless); `golden_frames.rs` runnable, baseline PNG tracked and current (~9 weeks old); `dhat-heap` (the only test-gating feature flag) runs in a dedicated CI job for both gated files (closes #1763, still holds); zero commented-out assertions workspace-wide; `assert!(...is_ok())`-only patterns (8 hits) are all either paired with follow-up value assertions or testing validators whose entire contract is `Ok(())`/`Err`; prior #1320/#1763/#1063/#1058 closures spot-checked, all held.

---

## Deferred

None. Every finding across all 9 dimensions is actionable now; none are
gated on an in-progress milestone (consistent with each dimension's own
"Deferred: None" notes).

---

## Notes on This Report's Compilation

- Compiled from 9 dimension sub-agent reports (`/tmp/audit/tech-debt/dim_1.md`
  through `dim_9.md`), dimensions 1/2/3/5 run in an earlier session segment,
  dimensions 4/6/7 and 8/9 run as two batches of ≤3 concurrent sub-agents in
  this segment per the skill's concurrency cap. All nine were confirmed
  read in full before compilation; the scratch directory's intermediate
  files were unexpectedly cleared mid-session (only `dim_8.md` survived on
  disk at compile time) — this report was assembled from the verbatim
  content captured during each dimension's review, not re-derived.
- **TD6-001 / PAT-D6-01 cross-reference**: both describe the same Skyrim+
  RACE `DATA` gap (`crates/plugin/src/esm/records/actor/mod.rs`), found
  independently by this audit (Dimension 6, stub/placeholder lens, LOW) and
  by `AUDIT_LEGACY_COMPAT_2026-08-07.md` (per-game translation-survey lens,
  MEDIUM). File one GitHub issue, not two — under PAT-D6-01's MEDIUM
  severity, since it reflects the real gameplay/compat blast radius more
  directly than this report's narrower "is this a stub" framing.
- **Filename note**: this report is saved as `AUDIT_TECH-DEBT_2026-08-07.md`
  (hyphenated) per explicit instruction, matching the on-disk convention of
  its two most recent predecessors (`AUDIT_TECH-DEBT_2026-07-16.md`,
  `AUDIT_TECH-DEBT_2026-08-03.md`) rather than the underscore convention the
  skill itself specifies — see TD4-003 above, which flags exactly this
  inconsistency as its own finding. Not resolved here; noted for whoever
  actions TD4-003's suggested batch rename.

Suggested next step: `/audit-publish docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`.

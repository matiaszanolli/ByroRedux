# Tech-Debt Audit — 2026-08-30

**Depth**: deep · **Dimensions**: all 9 · **Sweep**: part of a
`/audit-suite --preset comprehensive` run · **HEAD**: `64f64480` ·
**Delta**: 45 commits since `AUDIT_TECH_DEBT_2026-08-27.md` (`969d81c8`).

## Scope & method

Whole workspace (25 crates + `byroredux/`) plus the audit infrastructure itself
(`.claude/commands/`, `_audit-validate.sh`), which `_audit-common.md` places in scope.

**Executed by one agent, all nine dimensions in-process** — no sub-agent fan-out, per
the dispatch's explicit constraint (`feedback_audit_suite_nested_agent_relay`: an
orchestrator cannot retrieve its own sub-agents' results and then silently reports "no
findings" for dimensions it never consolidated). Each dimension was written to
`/tmp/audit/techdebt/dim_<N>.md` as it completed, then merged.

All analysis is static: `grep`, `git log -S` / `git blame`, `gh issue`, the validate
gate, and the SKILL's own `prod_loc` helper. **No cargo invocation and no engine
launch** — the run was under a hard memory constraint (an earlier audit in this suite
was OOM-killed).

**Deconflicted** against the 16 sibling audit reports already landed for 2026-08-30.
Two skill-drift items found by `/audit-renderer` (REN-2026-08-30-D1-02) and one by
`/audit-speedtree` are counted in the corpus tally but **not** re-filed here.

## Executive Summary

**25 findings: 0 CRITICAL · 0 HIGH · 7 MEDIUM · 18 LOW.**

Three dimensions produced **no findings at all**, and that is the headline as much as
the findings are:

- **Dimension 5 (stale markers): zero.** All 20 `TODO|FIXME|HACK|XXX` grep hits are
  documented false positives — 14 are the ESM `XXXX` extended-size protocol tag, 3 quote
  a *reference implementation's* FIXME, 1 is prose asserting the absence of a marker, 1
  is a closure note. **There is not one live marker in the codebase**, production or
  shader. The convention of writing deferrals into a doc comment with an issue number
  instead of a bare marker is holding completely.
- **Dimension 6 (stubs): zero fileable.** `unimplemented!` / `todo!()` / `panic!("not `
  → **0 hits**. No console command no-ops. Two documented deferrals noted only.
- **Dimension 9 (test hygiene): one LOW, and it is about *format*, not correctness.**
  All 169 `#[ignore]`s triaged: every one is legitimately gated on game data, a Vulkan
  or audio device, a release build, or is a calibration bench. Zero commented-out
  assertions. Zero vacuous tests (27 candidates read, all dismissed).

**The single most actionable item this cycle is not in the code — it is in the audit
corpus itself.** Twelve confirmed stale premises across nine of the thirty audit skill
files (Dimension 4), three of which would, followed literally, steer an auditor *away*
from a real defect or *toward* a phantom one. The path gate cannot see any of it: every
symbol resolves, every count is right, and the drift is purely semantic — claims that
outlived the code they describe. See "The skill-corpus problem" below.

The code-side picture is healthy and improving: `allow(dead_code)` fell **69 → 45**
(−35 %) and not one of the 45 survivors is unexplained. The two genuine code-debt items
of size are a 73-file scratch-example accumulation that violates the project's own
documented convention (TD8-…-01) and a 128-field God Object that CLAUDE.md Invariant 1
forbids (TD1-…-01).

### The skill-corpus problem

Six sibling audits this cycle independently reported that their *own* skill file carries
a stale checklist premise. This audit treated the corpus as a first-class doc-rot surface
and quantified it:

| | |
|---|---|
| Files in corpus | 30 (28 `audit-*/SKILL.md` + 2 shared `_audit-*.md`) |
| Path gate result | **GREEN** — 0 STALE across 2 305 refs / 99 files |
| Dimension-count claims | **all 19 in sync** |
| GPU struct sizes across corpus | **all in sync** (160 / 368 / 432 B) |
| Crate→owner map | covers all 25 live crates |
| **Confirmed semantic drift (this audit)** | **12 items / 9 files** |
| Confirmed by sibling audits, not re-filed | 3 items (`audit-renderer` ×2, `audit-speedtree` ×1) |
| **Corpus-wide this cycle** | **15 items across 9 of 30 files (30 %)** |

Three of the twelve are not merely inaccurate — they invert the auditor's task:

1. **`audit-save` Dim 1** says `ReferenceEnableState` has *"no consumer anywhere in
   cell_loader/streaming yet"* and instructs *"don't raise it as a save finding"*. It has
   had a consumer since #3278, with a dedicated regression test file.
2. **`audit-character` Dim 5** asserts an FO3↔FNV ruleset collapse that was removed, and
   tells the auditor that if the coefficients differ *"the collapse is wrong and every
   FO3 NPC is mis-statted"*. They do differ — by design (health 210 vs 205). The
   checklist manufactures a CRITICAL against correct code.
3. **`audit-speedtree` Dim 3** contradicts `audit-speedtree` Dim 5 on the billboard
   clamp; a top-down auditor reads the stale one first and accepts the exact NaN bug
   #3529 fixed.

`/audit-renderer` documented the same mechanism producing a *positive false statement in
the audit record*: its SKILL's "recast, don't re-report" instruction turned a
closed-on-2026-08-16 gap into a "re-verified as unchanged" line in the 2026-08-27 report.

**The structural point**: `_audit-validate.sh` was itself born as a `TD7-*` finding, and
it works — it checks that every backticked path and symbol *resolves*. Nothing checks
whether the *sentence around it* is still true. Twelfth instance found this cycle is in
`audit-tech-debt/SKILL.md` — the file this audit ran from (TD4-2026-08-30-12).

### Delta vs `AUDIT_TECH_DEBT_2026-08-27.md`

| Metric | 08-27 | 08-30 | Δ |
|---|---|---|---|
| TODO/FIXME/HACK/XXX | 20 (0 real) | 20 (0 real) | — |
| `allow(dead_code)` | 69 | **45** | **−24** |
| `unimplemented!`/`todo!()` | 0 | 0 | — |
| `#[ignore]` (incl. reason form) | 155 | 169 | +14 |
| Files > 2000 **production** LOC | 5 | 5 | — (same membership) |
| Files > 2000 total LOC | 27 | 30 | +3 |
| Committed `_tmp_*` examples | not measured | **73** | new metric |

The `allow(dead_code)` drop is the standout: −24 in three days, and the survivors are
now *uniformly* annotated with a rationale or `cfg`-gated. That sub-dimension is done.

## Baseline Snapshot (for the next audit's diff)

Measured at `64f64480` with the SKILL's Phase-1 recipes verbatim.

```
TODO/FIXME/HACK/XXX:                  20   (0 real — all ESM XXXX protocol,
                                            upstream-FIXME quotes, or prose)
allow(dead_code):                     45   (was 69; −24, none unexplained)
unimplemented!/todo!():                0
#[ignore] (^\s*#\[ignore, *.rs):     169   (33 with `= "reason"`, 136 bare)
files >2000 production LOC:            5   (draw.rs 3619, volumetrics.rs 2859,
                                            context/mod.rs 2468, mesh.rs 2207,
                                            texture_registry.rs 2013)
files >2000 total LOC:                30   (secondary bucket; none of the other
                                            25 crosses 2000 production)
functions >200 LOC:                   60   (7 live production offenders)
committed _tmp_* example targets:     73   (of 164 total examples; 6 978 LOC)
hardcoded Steam data-path literals:  119   (7 distinct game roots)
audit skill corpus:                   30 files, 12 confirmed stale premises
_audit-validate.sh:                   GREEN (0 STALE / 2 305 refs)
```

## Top 10 Quick Wins (trivial / small effort)

1. **TD4-2026-08-30-01** — fix `audit-save` Dim 1's `ReferenceEnableState` "no consumer"
   line. One paragraph; it is currently telling auditors not to look. *(trivial)*
2. **TD4-2026-08-30-02** — delete `audit-character` Dim 5's FO3↔FNV collapse bullet.
   Prevents a phantom CRITICAL. *(trivial)*
3. **TD4-2026-08-30-08** — reconcile `audit-speedtree` Dim 3 line 169 with its own line
   243 on the billboard clamp. *(trivial)*
4. **TD3-2026-08-30-01** — flip `docs/feature-matrix.md`'s Terrain LOD row; multi-band
   selection and `.btr` normal maps both shipped, and `audit-legacy-compat/SKILL.md`
   already says so. *(trivial)*
5. **TD8-2026-08-30-01** — `git rm` the 73 `_tmp_*` scratch examples and add
   `crates/*/examples/_tmp_*` to `.gitignore`. Removes 73 link steps from every CI run
   *and* makes the project's documented "deleted after use" convention self-enforcing.
   *(small — the highest compile-time payoff available)*
6. **TD4-2026-08-30-03/-09/-10** — three stale issue-state claims: `parse_refr_group`
   (closed `fa511bbf`), #1822 (closed), #746/#747 (closed, and never truncation-tail
   trackers). *(trivial each)*
7. **TD2-2026-08-30-02** — move the byte-identical 64-entry `BLUE_NOISE_RANKS` table into
   the `shader_constants.glsl` header both shaders already `#include`. *(trivial)*
8. **TD8-2026-08-30-02** — delete the two dead `pub fn` NPC-spawn shims, which forces 11
   doc comments to be re-pointed at the live `NpcSpawnJob`. *(small)*
9. **TD3-2026-08-30-02** — split the ACTI "Runtime consumer gap (M47.0)" block:
   `script_form_id` is live, the two sound fields are not. *(trivial)*
10. **TD4-2026-08-30-11** — anchor `audit-renderer`'s `struct GpuInstance` lockstep grep
    to `^struct` so it stops matching a comment and returning 6 for an expected 5.
    *(trivial)*

## Top 5 Medium Investments

1. **TD1-2026-08-30-01 — decompose `VulkanContext`'s 128 fields into five
   lifetime-scoped sub-structs.** This is the only place CLAUDE.md's "No God Objects"
   invariant is not held, and it is why `context/mod.rs` keeps returning to this report
   despite 13 sibling files and 17 401 LOC of prior extraction: the *behaviour* moved,
   the *state* did not. Grouping by destroy-order also makes reverse-order teardown
   locally checkable. *(large — one commit per sub-struct)*
2. **TD2-2026-08-30-01 — finish #1058.** `test_paths.rs` centralised game-data paths but
   is `pub(crate)`, so the integration tests in its own package structurally cannot use
   it and re-hardcode the same Steam roots 42 times (119 workspace-wide). Promote to a
   `crates/test-paths` dev-dependency, as that module's own doc anticipates. *(medium)*
3. **TD7-2026-08-30-01 — strengthen the shader-constant gate from a name allowlist to a
   structural rule.** Three RT reach budgets in `water.frag` bypass
   `shader_constants_data.rs` entirely, one carrying a `// matches triangle.frag` claim
   that is false. The existing gate can only catch redeclarations of names someone
   enumerated, so it will keep missing new ones by construction. *(medium)*
4. **TD1-2026-08-30-02 — split `texture_registry.rs` by lifecycle phase** (lookup /
   upload queue / release + deferred destroy). 2013 of 2021 lines are production, and
   the method ordering already implies the seam. *(medium)*
5. **TD9-2026-08-30-01 — convert the 136 bare `#[ignore]`s to the reason form.** Purely
   mechanical (the reason is in the doc comment one line up), and it makes this
   dimension's triage a `uniq -c` instead of 169 manual reads. Two prior audit
   miscounts (#3440, #3456) trace to this population not being self-describing.
   *(small)*

## A note on what this audit structurally could not close

`/audit-speedtree` this cycle falsified a premise that **both the skill and the
production code assert** — the 5-float Oblivion CNAM tier — by measuring 142/142 vanilla
records. Static cross-checking finds *disagreements* between doc and code; it cannot
find a claim both got wrong from the same upstream source. Any premise about on-disk
data shape is therefore outside a static run's reach, and this report marks those as
unclosed rather than "verified". Details in Dimension 4.

## Deferred

- **Corpus-shape premises** (per the note above) — gated on a run with game data
  mounted; not a milestone dependency.
- **`crates/mod-runtime` (1 475 LOC, zero consumers)** — deliberate, documented in
  `_audit-common.md`'s un-owned table, gated on the sandboxed-mod host milestone. Only
  its dangling `[workspace.dependencies]` alias is filed (TD8-2026-08-30-03).
- **`ImgsRecord.dnam_raw`** — declared parser-side capture, gated on #624 /
  SK-D6-NEW-03's consumer.

---

# Findings

Ordered MEDIUM → LOW, then by dimension. Full per-dimension detail follows.

## MEDIUM (7)

| ID | Dimension | Summary |
|---|---|---|
| TD4-2026-08-30-01 | 4 Audit rot | `audit-save` Dim 1 says `ReferenceEnableState` has no consumer and tells the auditor not to file — false since #3278 |
| TD4-2026-08-30-02 | 4 Audit rot | `audit-character` Dim 5 asserts a removed FO3↔FNV ruleset collapse and steers toward a phantom CRITICAL |
| TD4-2026-08-30-03 | 4 Audit rot | `audit-esm` Dim 5 names `parse_refr_group` "the live regression case" — closed in `fa511bbf` |
| TD8-2026-08-30-01 | 8 Dead code | 73 committed `_tmp_*` scratch examples (6 978 LOC, 45 % of all example targets), compiled twice per CI run, linted never, still growing |
| TD1-2026-08-30-01 | 1 Complexity | `VulkanContext` is a 728-line, 128-field God Object — the reason `context/mod.rs` will not stay split |
| TD2-2026-08-30-01 | 2 Duplication | `test_paths.rs` is `pub(crate)`, so its own package's integration tests re-hardcode 42 Steam paths (119 workspace-wide) |
| TD7-2026-08-30-01 | 7 Magic numbers | `water.frag`'s three RT reach budgets bypass `shader_constants_data.rs`; `// matches triangle.frag` is false |

## LOW (18)

| ID | Dimension | Summary |
|---|---|---|
| TD4-2026-08-30-04 | 4 | `audit-ecs` cites `write_lazy!` + `ensure_subtree_cache`, both deleted by #2399 |
| TD4-2026-08-30-05 | 4 | `audit-ecs` calls the boot guard `debug_assert_eq!`; it is release-level `assert_eq!` (#2690) |
| TD4-2026-08-30-06 | 4 | `audit-scripting` enumerates 5 of 7 VMAD base-record families (statics + terminals added by #2663) |
| TD4-2026-08-30-07 | 4 | `audit-scripting` says locals are never traced to their aliased property; `scope.object_locals` does exactly that |
| TD4-2026-08-30-08 | 4 | `audit-speedtree` Dim 3 and Dim 5 contradict each other on the billboard clamp |
| TD4-2026-08-30-09 | 4 | `audit-speedtree` calls #1822 "the one that remains open" — CLOSED |
| TD4-2026-08-30-10 | 4 | `audit-starfield` points the truncation-tail check at #746/#747, both closed and neither a tail tracker |
| TD4-2026-08-30-11 | 4 | `audit-renderer`'s prescribed `GpuInstance` lockstep grep returns 6 for a stated 5 (matches a comment) |
| TD4-2026-08-30-12 | 4 | `audit-tech-debt`'s own Dim 2 names the pre-#1044 coord home, inverting the leak test |
| TD2-2026-08-30-02 | 2 | The 64-entry `BLUE_NOISE_RANKS` table is byte-identical in two shaders that already share an include |
| TD3-2026-08-30-01 | 3 | `docs/feature-matrix.md` calls shipped distance-based multi-band LOD "deferred"; a sibling skill already contradicts it |
| TD3-2026-08-30-02 | 3 | ACTI's "Runtime consumer gap (M47.0)" block describes a gap M47.0 closed (half-true, which is worse) |
| TD1-2026-08-30-02 | 1 | `texture_registry.rs` is 2013 production LOC in a 2021-line file — filed against #3081's non-reproducible 838 figure |
| TD1-2026-08-30-03 | 1 | `recreate_screen_passes` is 700 LOC, in the file whose 761-LOC predecessor #1671 already split |
| TD1-2026-08-30-04 | 1 | `build_scheduler` is 818 LOC — one registration wall, splittable per `Stage` |
| TD8-2026-08-30-02 | 8 | Two dead `pub fn` NPC-spawn shims with 11 doc comments pointing readers at them |
| TD8-2026-08-30-03 | 8 | `byroredux-mod-runtime` is a dangling `[workspace.dependencies]` alias |
| TD9-2026-08-30-01 | 9 | 80 % of `#[ignore]`s carry no machine-readable reason; already produced two audit miscounts |

## Dedup — existing OPEN issues re-verified, not re-filed

| Issue | Verdict |
|---|---|
| #3282 `draw_frame` | Filed at 2498 LOC; **now 2521** (+23 while open). Add as a data point. |
| #3451 `mesh.rs` | Filed at 2049 production; **now 2207** (+158). |
| #2256 `volumetrics.rs` | Still over at 2859 production. |
| #2257 `material.rs` | **False positive confirmed** under `prod_loc` (~1440 production). Recommend closing. |
| #1761 `Dx10Chunk` | Premise re-verified true: `start_mip` **is** read (`ba2.rs:683,688,692,697`), so its attribute is redundant; `end_mip` set-never-read. |
| #3150 TEMP scratch examples | Scoped to 3 `crates/plugin` probes; the real population is **73**. Recommend re-scoping rather than filing a near-duplicate (see TD8-2026-08-30-01). |
| #3450 GpuCamera 352 B | Subject already fixed in-corpus (all sizes in sync). Triage-hygiene only. |
| #3476 raw version comparisons | Different parser; `crates/nif/src/blocks/` is clean (0 raw hex version comparisons). |

---

# Dimension 1: File / Function / Module Complexity

## Primary bucket (production LOC > 2000) — re-run with `prod_loc`, 5 members

| Prod LOC | Total | File | Status |
|---|---|---|---|
| 3619 | 4959 | `crates/renderer/src/vulkan/context/draw.rs` | **#3282 OPEN** (dedup) |
| 2859 | 3745 | `crates/renderer/src/vulkan/volumetrics.rs` | **#2256 OPEN** (dedup) |
| 2468 | 3024 | `crates/renderer/src/vulkan/context/mod.rs` | **unfiled → TD1-…-01** |
| 2207 | 3164 | `crates/renderer/src/mesh.rs` | **#3451 OPEN** (dedup) |
| 2013 | 2021 | `crates/renderer/src/texture_registry.rs` | **unfiled → TD1-…-02** |

Membership is identical to the skill's 2026-08-29 orientation figures (±3 LOC), so the
set did not churn this cycle. Secondary bucket (total LOC > 2000): 30 files —
`prod_loc`-checked each; none of the 25 outside the table above crosses 2000 production
(largest: `crates/physics/src/world.rs` 2617 total, `crates/plugin/src/esm/records/misc/world.rs`
2578 total, `byroredux/src/env_translate.rs` 3242 total — all majority-test or
majority-doc). No secondary-bucket file is escalated.

## TD1-2026-08-30-01 — MEDIUM — `VulkanContext` is a 728-line, 128-field struct: a God Object against CLAUDE.md Invariant 1

`crates/renderer/src/vulkan/context/mod.rs:1157` — `pub struct VulkanContext` spans
**728 lines and declares 128 fields**. It is 30 % of the file's 2468 production LOC on
its own, and it is the reason this file will not stay split: the `context/` directory
already has **13 siblings totalling 17 401 LOC** (`init.rs` 1642, `draw.rs` 4959,
`resize.rs` 1736, `post_passes.rs` 1190, …) — the *behaviour* has been extracted
repeatedly, but every extraction still reaches back into the same 128-field struct,
so the type stays a single mutable God Object and `mod.rs` stays over threshold.

This is a different defect from "file is long", and it is why the previous splits did
not settle it. CLAUDE.md Architecture Invariant 1 is explicit: *"ECS over scene graph.
Components are data, systems are logic. **No God Objects.**"* `VulkanContext` is the
one place in the engine that invariant is not held.

**Split axis — by resource lifetime, not by line count.** Group the 128 fields into
sub-structs that each own one destroy-order group, which also makes the reverse-order
teardown in `teardown.rs` locally checkable instead of a 128-field manual sequence:
- `SwapchainResources` — swapchain, images, views, framebuffers, depth, render pass
  (everything `recreate_swapchain` rebuilds).
- `RtResources` — `accel_manager`, TLAS/BLAS handles, ray-query descriptor state.
- `PostChain` — SVGF, TAA, composite, bloom, volumetrics, FSR/upscaler.
- `OverlayResources` — UI quad, egui bridge, screenshot/depth-capture handles.
- `Telemetry` — the `fill_*` accessors' backing counters (`fill_upscaler_telemetry`,
  `fill_scratch_telemetry`, `fill_skin_coverage_stats`, `fill_rt_integrity_stats`,
  lines 2068–2354, ~290 LOC, which can move wholesale with their data).

Then two further pure-code moves are free and independent of the struct work:
`DrawCommand` + `to_gpu_material` + `material_hash` (lines 410–885, ~475 LOC) →
`context/draw_command.rs`; the telemetry fillers → `context/telemetry.rs`. Those two
alone drop `mod.rs` under threshold.

**Method** (project convention, `feedback_safe_large_function_split`): `sed`-extract the
exact line ranges rather than retyping, and diff-check before committing because
`cargo fmt` reformats the whole crate. This is renderer-adjacent but **not** a
render-pass or barrier change — moving a struct field into a sub-struct does not touch
submission order, so `feedback_speculative_vulkan_fixes` does not gate it. Do not
reorder destroys while doing it.
Effort: large — decompose per sub-struct, one commit each.

## TD1-2026-08-30-02 — LOW — `texture_registry.rs` is 2013 production LOC in a 2021-line file (99.6 % production)

`crates/renderer/src/texture_registry.rs`. Unlike every other primary-bucket member,
this file is *not* inflated by inline tests — its own tests live in the sibling
`texture_registry_tests.rs` via `#[cfg(test)] #[path] mod`. 2013 of 2021 lines are
production texture-registry logic.

**This is filed deliberately against #3081's evidence table**, which recorded this file's
production LOC as 838 (majority-test). Re-checked directly: the file contains exactly 3
`#[cfg(test)]` markers, all within the last ~100 lines, two of which are
`#[path = "..."] mod` declarations pointing at *separate* files. The 838 figure is not
reproducible; 2013 is.

**Split axis — by lifecycle phase**, which the existing method ordering already implies:
- **Acquire / lookup** (`get_by_path*`, `acquire_by_path*`, `acquire_by_path_for_view`,
  `fallback`, `neutral_fallback`, lines 1131–1240) → `texture_registry/lookup.rs`.
- **Upload queue** (`enqueue_dds*`, `enqueue_dds_for_view`, `queue_or_hit*`,
  `pending_dds_upload_count`, `flush_pending_uploads`, lines 736–1130 — `flush_pending_uploads`
  alone is ~200 LOC) → `texture_registry/upload.rs`.
- **Release / deferred destroy** (`drop_texture`, `drop_textures`, `drop_released_texture`,
  `release_ref`, `release_refs_batch`, `decrement_ref`, `tick_deferred_destroy`,
  `drain_pending_destroys`, lines 1283–1430) → `texture_registry/release.rs`.
- The struct, `new()` (322–532) and the bindless descriptor plumbing stay in `mod.rs`.

Effort: medium. Same `sed`-extract method and `cargo fmt` caveat as above.

## TD1-2026-08-30-03 — LOW — `recreate_screen_passes` is 700 LOC, in the file whose 761-LOC predecessor was already split under #1671

`crates/renderer/src/vulkan/context/resize.rs:487`. #1671 (CLOSED) split
`recreate_swapchain` at 761 LOC into `recreate_swapchain_core` (now 332 LOC, line 32)
plus siblings. `recreate_screen_passes` has since grown to **700 LOC** in the same file
— 4 LOC short of what triggered the original split, and the same shape: one linear
rebuild of every screen-sized attachment and its dependent descriptor writes.

This is the identical regrowth pattern `draw_frame` has now repeated three times
(#1052 → #1748 → #1857 → #2197 → #2255 → #3282). Worth noting explicitly: closing a
function-split issue has not, historically, kept the function split.

**Split axis**: per pass group, mirroring the attachment families the function rebuilds
— G-buffer attachments / SVGF + TAA history / composite + bloom chain / upscaler
inputs. Each group is an independent `create → transition → write descriptors` triple.
Effort: medium. This one **is** render-pass adjacent: it recreates attachments and
rewrites descriptor sets, so per `feedback_speculative_vulkan_fixes.md` do not change
layout-transition order or barrier placement while moving code, and validate under
`BYRO_VALIDATION=1` rather than on `cargo test` alone.

## TD1-2026-08-30-04 — LOW — `build_scheduler` is 818 LOC: one registration wall in a 1797-production-LOC `boot.rs`

`byroredux/src/boot.rs:706`. The skill already names `boot.rs` "the single
scheduler-registration wall"; the measurable form of that is one 818-LOC function
listing every `add_to_with_access` / `add_exclusive` call in stage order, plus the
three release-level `assert_eq!` access-report guards at its tail (lines 1541–1563).

Not urgent — a flat registration list is legitimately linear and the access-report
assertions make mistakes loud at boot. But at 818 LOC it is the largest non-renderer
function in the workspace, and a stage-ordering mistake inside it is reviewed by eye.
**Split axis**: one `register_<stage>_systems(&mut scheduler)` per `Stage`
(`Early`/`Update`/`PostUpdate`/`Physics`/`Late`), with `build_scheduler` reduced to five
calls plus the guard block — which also makes the stage-ordering test at
`crates/core/src/ecs/scheduler.rs` map 1:1 onto five reviewable functions.
Effort: small (mechanical, `sed`-extract per stage).

## Dedup — existing OPEN issues, re-verified, not re-filed

- **#3282 `draw_frame`**: filed at 2498 LOC / 4909-line file. **Today: 2521 LOC / 4959-line
  file.** It has grown a further +23 LOC while the issue sat open. Worth adding to the
  issue as a data point rather than filing again.
- **#3451 `mesh.rs`**: filed at 2049 production. Today **2207** — +158 since filing.
- **#2256 `volumetrics.rs`**: filed on the crossing. Today 2859 production.
- **#2257 `material.rs`**: confirmed a **false positive** under `prod_loc`
  (2333 total, ~1440 production) — matches the skill's note. Recommend closing.

## Secondary checks — all clean or explicitly dismissed

- **Functions > 200 LOC**: 60 workspace-wide. After excluding tests, examples and build
  scripts, the four above plus `about_to_wait` (629, `app_events.rs:490`),
  `render_one_frame` (613, `app_frame.rs:48`) and `animation_system_inner`
  (488, `systems/animation.rs:514`) are the live production offenders. The latter three
  are per-frame drivers whose length is dispatch breadth, not nesting; noted, not filed.
- **`mod.rs`/`lib.rs` with > 20 `pub use`**: exactly one —
  `crates/core/src/ecs/components/mod.rs` (39). A component re-export hub's job *is*
  re-exporting; it carries no logic. **Not debt.**
- **Match arms > 50** — 4 sites, all examined, **none filed**:
  - `byroredux/src/ui_input.rs:234` (134 arms) and `:375` (80 arms) — 1:1
    `winit::KeyCode` → `UiPhysicalKey` and `NamedKey` → `UiNamedKey` translation.
    Converting these to a lookup table would **discard the compiler's exhaustiveness
    check**, which is the only thing catching a winit enum gaining a variant. The naive
    ">50 arms → lookup table" rule is wrong here; recording that so a future audit
    does not file it.
  - `crates/nif/src/blocks/mod.rs:302` (202 arms) — the NIF block dispatcher, `&str` →
    boxed parse. A `&str` match has no exhaustiveness guarantee, so a fn-pointer table
    *is* mechanically viable here. Not proposing it: CLAUDE.md tracks this arm count as
    a coverage metric, each arm carries per-block provenance comments (issue numbers,
    corpus counts), and `nif_shape_dispatch_resolve_parity` requires every dispatch arm
    to keep a matching `resolve_shape` arm — a table indirection would make that parity
    harder to audit, not easier. Cost > benefit.
  - `crates/papyrus/src/token.rs:345` (82 arms) — logos token → keyword mapping,
    same exhaustiveness argument as `ui_input.rs`.
# Dimension 2: Logic Duplication

## TD2-2026-08-30-01 — MEDIUM — the game-data path helper built to end path hardcoding cannot be reached by the tests that hardcode paths

`crates/plugin/src/esm/test_paths.rs` was created by **#1058** for exactly this. Its own
module doc states the intent:

> "Pre-#1058 each test hardcoded the audit author's Steam install path; this module
> centralises the override shape so every test resolves the same way."

It provides 12 `pub(crate) fn` accessors, each an env-var override
(`BYROREDUX_<GAME>_DATA`) falling back to the reference machine's Steam path.

**It is declared `pub(crate) mod test_paths;` (`crates/plugin/src/esm/mod.rs:14`).** An
integration test under `tests/` is a *separate crate*, so
`crates/plugin/tests/parse_real_esm.rs` — in the same package — structurally cannot call
it. The result: that one file re-hardcodes the same Steam roots **42 times**.

Workspace-wide the literal `"/mnt/data/SteamLibrary/steamapps/common/..."` appears
**119 times** across `crates/plugin`, `crates/nif`, `crates/bsa`, `crates/spt`,
`crates/audio`, `crates/facegen`, `crates/sfmaterial` and `byroredux/tests`, covering
7 distinct game roots (FNV ×29, Skyrim SE ×18, FO4 ×14, Oblivion ×13, FO3 ×10,
Starfield ×8, FO76 ×2, plus bare-`common` ×10).

**Amplification** (why this is not the default LOW): this is duplicated logic with
*divergent* behaviour, not just repeated text. `test_paths.rs` guarantees every accessor
consults its env override first; the 119 open-coded sites each re-implement that
override by hand — some do (`std::env::var("BYROREDUX_FNV_DATA").unwrap_or(...)`), and
whether *all* do is unverifiable by inspection at that scale. A site that forgets the
env var is a test that silently skips on any machine but one, which is the failure mode
`#1058` set out to remove and did not finish removing.

**Consolidation site** — named, and the module already anticipates it: `test_paths.rs`'s
own doc says *"promoting to a workspace-level utility crate is out of scope for the
issue that introduced this module (#1058)"*. That is the fix, one increment later.
Two options, in order of preference:
1. A tiny `crates/test-paths` dev-dependency crate carrying the 12 accessors plus the
   `nif/tests/common::Game` `default_path()` / `mesh_archive()` convention it already
   mirrors. Every crate lists it under `[dev-dependencies]`; all 119 literals collapse
   to 7 constants in one file.
2. Cheaper interim: change `pub(crate) mod test_paths` to
   `pub mod test_paths` gated behind a `test-paths` feature enabled in the plugin
   crate's own `[dev-dependencies]` self-reference — unblocks `parse_real_esm.rs`'s 42
   sites immediately without touching the other crates.
Effort: medium (option 1), small (option 2).

## TD2-2026-08-30-02 — LOW — the 64-entry blue-noise rank table is duplicated verbatim across two shaders that already share an include

`crates/renderer/shaders/composite.frag:258-267` and
`crates/renderer/shaders/volumetrics_inject.comp:1246-1255` each declare:

```glsl
const uint BLUE_NOISE_RANKS[64] = uint[64](
     0u, 41u, 11u, 59u,  2u, 40u, 10u, 32u,
    ...
    63u, 16u, 57u, 27u, 37u, 19u, 56u, 30u
);
```

Diffed line-by-line: the two tables are **byte-identical**. Only the consuming function
differs (`preResolveDither()` vs `blueNoiseRank(ivec2, int)`), and each consumer's
tiling offsets are its own business — the *table* is the shared constant.

**The consolidation site already exists and both files already use it**: both
`#include "include/shader_constants.glsl"` (composite.frag:8, volumetrics_inject.comp:33).
Move the array into a header — either `shader_constants.glsl` via
`crates/renderer/src/shader_constants_data.rs` (which is the documented single source
for every shader constant, `include!`d by both `shader_constants.rs` and `build.rs`) or a
new `include/blue_noise.glsl` — and delete both copies.

An 8×8 void-and-cluster rank table is exactly the kind of value that must never diverge:
if one copy is regenerated and the other is not, the composite dither and the froxel
jitter fall out of phase and produce correlated banding that looks like a denoiser bug,
not a constants bug. Effort: trivial.

## Checked, no finding

- **Z-up → Y-up coordinate flips.** Swept for open-coded `[.x, .z, -.y]` swizzles outside
  the canonical home. **Zero leaks.** Every one of the ~15 conversion sites
  (`nif/import/mesh/tangent.rs`, `sse_recon.rs`, `spt/import/mod.rs`,
  `nif/import/precombine.rs`, `nif/anim/{transform,keys,bspline}.rs`,
  `core/animation/root_motion.rs`) routes through
  `byroredux_core::math::coord::zup_to_yup_pos` / `zup_to_yup_quat_wxyz`. This is a
  **fully converged** consolidation (#1044 / TD3-002) — worth recording as a success so a
  future audit does not re-open it. *(The audit-tech-debt SKILL still names the pre-#1044
  homes — filed under Dim 4.)*
- **`crates/nif/src/anim/coord.rs`** is a 14-line `pub use` re-export shim. Considered as
  dead back-compat cruft (Dim 8) and **dropped**: it is live, providing the shorter
  `zup_to_yup_quat` alias to `anim/{transform,keys,bspline}.rs` (12 call sites). Not rot.
- **NIF block-parser scaffolding**: the per-block `header read → field read → fixup`
  shape does repeat across `crates/nif/src/blocks/`, but each block's field list is its
  own wire schema and the repetition is the schema, not logic. No shared helper would
  remove a decision. Not filed.
- **`vk::WriteDescriptorSet` boilerplate** and per-pass image-layout barrier sequences:
  the previously-filed instance (#2073, bindless `COMBINED_IMAGE_SAMPLER` written twice
  in `texture_registry.rs`) is **CLOSED** and the duplicate is gone. No new pair found.
# Dimension 3: Stale Documentation & Comments

Path gate `_audit-validate.sh`: **GREEN**, 0 STALE across 2305 refs / 99 files. So there
are no free trivial findings this cycle — everything below is content rot the gate
cannot see.

## TD3-2026-08-30-01 — MEDIUM — `docs/feature-matrix.md` calls shipped distance-based LOD "deferred", and a sibling skill already contradicts it

`docs/feature-matrix.md:54`:

> **Terrain LOD (M35)** | ~ Partial | `.btr` (Skyrim+/FO4) + `.bto` + `_far.nif`
> (Oblivion/FO3/FNV) shipped; **distance-based multi-band selection** + `.btr` normal
> maps deferred

Both halves are stale:

- **Distance-based multi-band selection shipped.** `byroredux/src/cell_loader/lod_bands.rs`
  (837 LOC) implements the per-game distance ladder — `LodBandLadder::for_game` /
  `for_terrain_game` / `for_object_game`, `LodBandSelection`, `select_lod_quads`,
  `quad_min_chebyshev` — and it is consumed by **both** `terrain_lod.rs:265,303,382` and
  `object_lod.rs:140,151`. `object_lod.rs:472` records "All of them are streamed since
  **#2371**". It was extended twice more since: **#3321** (`e23a9908`, 2026-08-27, consume
  FNV/FO3 distant-object LOD) and **#3385** (`c7a70d45`, memoise the archive-presence probe).
- **`.btr` normal maps shipped.** `byroredux/src/cell_loader/terrain_lod_btr.rs:121,138`
  — `btr_normal_path`, "Per-quad **tangent-space** normal map for a distant-terrain
  `.btr`", including the FO4 `_msn` handling the same doc block documents.

**The amplification is concrete and recent**: `.claude/commands/audit-legacy-compat/SKILL.md:186`
already carries the corrected statement — "*is NO LONGER a gap (#3321, `e23a9908`,
2026-08-27) — do not re-file it*". So the skill corpus was updated on 2026-08-27 and the
feature matrix was not. Two designated-authoritative documents now disagree, and the
matrix is the one a reader reaches first. Per the skill's own framing, the matrix is a
"status floor, not a record of what exists" — but a floor that reads *lower* than
reality invites re-implementation of work already done.
Fix: flip the row to ✓ (or ~ with the two genuine remainders named). Effort: trivial.

## TD3-2026-08-30-02 — LOW — the ACTI record's "Runtime consumer gap (M47.0)" block describes a gap M47.0 closed

`crates/plugin/src/esm/records/misc/world.rs`, doc block above `ActiRecord` (~line 1287):

> **Runtime consumer gap (M47.0):** the captured `script_form_id` / `sound_form_id` /
> `radio_form_id` cross-refs **ride through unused today**; the trigger / event-hook
> runtime **planned for M47.0** will dispatch ActivateEvent to the SCRI-linked script …
> Until then the stub closes the parser-side silent drop so the M47.0 work has one grep
> target.

`script_form_id` is consumed: `ActiRecord` is the first arm of
`EsmIndex::base_record_script` (`crates/plugin/src/esm/records/index.rs:738-740`), which
`byroredux/src/cell_loader/references/attach.rs:239-248` calls to resolve
`index.scripts.get(&script_form_id)` — the attach path even logs with an `"M47.0: "`
prefix, i.e. M47.0 shipped. The field-level doc at `ActiRecord.script_form_id`
("Referenced by trigger-system dispatch **once it lands**") is the same drift restated.

`sound_form_id` and `radio_form_id` **are** still unconsumed (verified: no reader outside
`records/`), so the paragraph is half-true — which is worse than wholly stale, because a
reader cannot tell which half to trust. Fix: split it — say `script_form_id` is live via
`base_record_script` since M47.0, and keep the deferral note for the two sound fields
only. Effort: trivial.

## Verified CLEAN — previously-recurring rot that is now fixed

- **`Material::classify_pbr` doc rot** (the skill's named recurring trap). All 8 prose
  references in `crates/core/src/ecs/components/material.rs` (lines 718, 1125, 1283,
  1352, 1974, …) now frame it correctly as *deleted / historical* — "the per-draw fallback
  that was **removed** in the NIFAL canonical-material-translation refactor", "the
  (deleted per-draw) `classify_pbr`", "the hard-coded lists in the (deleted)
  `Material::classify_pbr` and now in `classify_pbr_keyword` (**the surviving free
  function**)". The live symbols `classify_pbr_keyword` (line 898) and
  `Material::resolve_pbr` (line 1165) are the ones the prose points at. **No finding** —
  recording it so the next audit does not re-derive it.
- **GPU struct byte sizes in doc comments.** Cross-checked every size claim in
  `crates/renderer/src/vulkan/material.rs`, `scene_buffer/constants.rs` and
  `context/mod.rs` against the authoritative tests: `GpuInstance` 160 B
  (`gpu_instance_is_160_bytes_std430_compatible`), `GpuCamera` 368 B
  (`gpu_camera_is_368_bytes`), `GpuMaterial` 432 B (`gpu_material_size_is_432_bytes`).
  **All in sync**, including the test *names* matching their asserted values. The
  historically recurring `GpuCamera` drift (#1623 at 304 B, later 336→352) is closed.
- **ROADMAP.md**: zero milestones marked "in progress" / WIP, so no open/closed
  cross-check drift exists to file.
- **`crates/renderer/shaders/triangle.frag`** third-party attribution block
  (GLSL-PathTracer MIT + Burley 2012) intact and unmodified.
# Dimension 4: Audit-Finding Rot (skill corpus)

Corpus: 28 `.claude/commands/audit-*/SKILL.md` + 2 shared `_audit-*.md` = 30 files.
Path gate `_audit-validate.sh`: **GREEN** (2305 refs, 0 STALE). 5 symbol advisories in
audit skills, all false positives (GitHub label names `concurrency`/`enhancement`/
`speedtree`, finding-format field names `Related`/`Severity`).
Dimension counts (`### Dimension N` vs "Default: all N"): **all 19 skills in sync**.
GPU struct sizes across the corpus: **in sync** (GpuInstance 160 B, GpuCamera 368 B,
GpuMaterial 432 B) — #3450's subject is already fixed in-corpus though the issue is open.
Crate→owner map covers all 25 live crates; un-owned table says "Seven" and has 7 rows;
LOC claims for main.rs (1053), studio_host.rs (252), combat.rs (952), sdk (282),
platform (60) all verified accurate.

**So the drift is purely SEMANTIC** — every symbol resolves, every count is right, but
the *claims about* those symbols have gone stale. The path gate is structurally blind
to this class.

## Confirmed drift: 12 items across 9 of 30 files (30%)

A further 3 items in the same corpus were confirmed by concurrent sibling audits this
cycle and are **not** re-filed here: `audit-renderer` Dim 1 ×2 (REN-2026-08-30-D1-02 — a
deleted `build_blas_for_mesh` entry point, and a "no recovery path exists" premise closed
on 2026-08-16) and `audit-speedtree` Dim 2 ×1 (a "vanilla Oblivion ships MODB-only"
premise falsified at 142/142 by corpus measurement). **Corpus-wide this cycle: 15
confirmed drift items across 9 of 30 files.**

### TD4-2026-08-30-01 — MEDIUM — audit-save Dim 1 tells the auditor NOT to look
`.claude/commands/audit-save/SKILL.md:219-225` asserts `ReferenceEnableState`
"has **no consumer anywhere in cell_loader/streaming yet** (`is_enabled` is called only
from its own test module)" and instructs: "don't raise it as a save finding, but don't
claim `Disable()` persists visibly either".

FALSE since `265f0c9b` ("Fix #3256, Fix #3278 … give Disable() a runtime consumer").
Live consumers: `byroredux/src/cell_loader/spawn.rs:458` (gates REFR spawn),
`spawn.rs:633` (log line), plus a dedicated regression file
`byroredux/src/cell_loader/reference_enable_gate_tests.rs`.
The bullet actively steers an auditor away from verifying the round-trip of a
component that now HAS observable live effect. Fix: reframe as "wired since #3278;
verify the spawn gate still reads it after a load-apply".

### TD4-2026-08-30-02 — MEDIUM — audit-character Dim 5 steers toward a phantom CRITICAL
`.claude/commands/audit-character/SKILL.md:307-310`: "`GameKind::Fallout3NV` resolves to
the **FNV** ruleset for both FO3 and FNV, justified because the actor-general derived
stats are identical. Verify that justification … if any actor-general coefficient
differs, the collapse is wrong and every FO3 NPC is mis-statted."

The collapse **no longer exists**. `crates/plugin/src/esm/records/mod.rs:149-153`
(`character_rules_profile`) splits on HEDR version:
`GameKind::Fallout3NV if hedr_version < 1.0 => CharacterRulesProfile::FALLOUT3`,
else `FALLOUT_NEW_VEGAS`. The two rulesets are demonstrably distinct —
`crates/core/src/character/profile.rs:184-185` dispatches `fallout3_ruleset` vs
`falloutnv_ruleset`, and the pinning test at `profile.rs:206-219` asserts
`fo3_health.evaluate(5.0, 2.0) == 210.0` vs `fnv_health … == 205.0`, plus
`SkillSet::FALLOUT3` vs `SkillSet::FALLOUT_NV`.
An auditor following this bullet finds the coefficients *do* differ and files
"every FO3 NPC is mis-statted" as a CRITICAL against code that is already correct.

### TD4-2026-08-30-03 — MEDIUM — audit-esm Dim 5 names a CLOSED defect as "the live regression case"
`.claude/commands/audit-esm/SKILL.md:339-343`: "`parse_refr_group` (same file, also an
entry point above) was **not** updated alongside them — it still recurses on
`reader.group_content_end(&sub)` with no depth counter. Verify whether that gap is
still open; **if so it is the live regression case for this bullet, not a hypothetical**."

Closed by `fa511bbf` ("Fix #3503 … bound the last 8 GRUP walkers", 2026-08-29).
`crates/plugin/src/esm/cell/walkers.rs:653-690` now splits into
`parse_refr_group` → `parse_refr_group_inner(…, 0)`, routes `sub_end` through
`reader.bounded_group_content_end(&sub, depth, "parse_refr_group")`, and threads
`depth + 1`. The bullet's own "if so" hedge saves it from being a hard misdirection,
but it burns auditor time and invites a duplicate filing.

### TD4-2026-08-30-04 — LOW — audit-ecs cites two symbols deleted by #2399
`.claude/commands/audit-ecs/SKILL.md:379-381`: "helpers `ensure_subtree_cache` /
`write_root_motion` / `apply_bool_channels` + the `write_lazy!` macro (5 color-target
arms) were factored out by `2bdbc36` — DRY-undo drift there is a finding."

`write_lazy!` and `ensure_subtree_cache` no longer exist anywhere in `crates`/`byroredux`
(`grep` → 0 hits each). Both were removed by `f46fcfd8` ("Fix #2399: fix
content-determined lock order in animation channel apply") — the macro was the *cause*
of the lock-order defect, so its removal was deliberate, not a DRY-undo.
`write_root_motion` and `apply_bool_channels` survive. Reading this bullet literally,
an auditor sees "the macro is gone" and files exactly the DRY-undo finding the bullet
invites — against an intentional fix.

### TD4-2026-08-30-05 — LOW — audit-ecs downgrades a release assertion to debug
`.claude/commands/audit-ecs/SKILL.md:189`: "`byroredux/src/boot.rs`
(`install_runtime_registries`) runs
`debug_assert_eq!(scheduler.access_report().undeclared_parallel_count(), 0)`".

`byroredux/src/boot.rs:1541-1563` uses release-level `assert_eq!` for all three guards
(`undeclared_parallel_count`, `known_conflict_count`, `unknown_pair_count`), with an
explicit comment: "Keep these as release assertions: schedule construction runs once,
and a release-only divergence must not ship without the proof" (#2690). An auditor
trusting the skill would flag the guard as debug-only — i.e. file a finding for a
property that was deliberately strengthened.

### TD4-2026-08-30-06 — LOW — audit-scripting enumerates 5 of 7 VMAD base-record families
`.claude/commands/audit-scripting/SKILL.md:1246-1257` describes
`index.rs::base_record_script_instance` as checking "ACTI/CONT/NPC/CREA base records in
order, then (#2189) the item family", and then instructs: "Verify the record types
covered match the VMAD-bearing set (a scripted base type not in the chain → its scripts
never attach)".

`crates/plugin/src/esm/records/index.rs:777-820` has **seven** arms — the five listed
plus, per #2663, `self.cells.statics` (the MODL-only world-placement family
STAT/MSTT/FURN/DOOR/LIGH/FLOR/IDLM/BNDS/ADDN/TACT) and `self.terminals` (FO4 ships 207
VMAD-bearing TERM records). Guards exist for both
(`base_record_script_instance_resolves_a_statics_familys_vmad`,
`…_resolves_a_terminals_vmad`). An auditor verifying "covered == VMAD-bearing set"
against the skill's list re-derives #2663 as a fresh gap.

### TD4-2026-08-30-07 — LOW — audit-scripting says locals are never traced back; they are
`.claude/commands/audit-scripting/SKILL.md:810-813`: "decline any local-variable
receiver, including a side-effect-free ident copy (`ObjectReference k = SomeProperty;
k.AddItem(...)`) — **this increment deliberately doesn't trace a local back to the
property it aliases**, so a local receiver must decline via
`scope.quest_locals`/`scope.decl_locals`, not silently resolve."

`crates/scripting/src/translate/effects.rs` now carries a third map,
`object_locals: HashMap<String, ObjectRef>` (line 238), populated at line 430 from
`Binding::Object(via)` and consulted first thing in `receiver_object` (line 1197) —
so an object-typed local **does** resolve. Introduced by `0ff8612b` (MQ101 cinematic
effects). The map is absent from the entire `.claude/commands/` corpus
(`grep object_locals .claude/commands/` → 0 hits). Same skill's line 190 claim
("`receiver_object` still declines a *local-variable* receiver … that specific decline
… remains correct") is the same drift stated a second time.

### TD4-2026-08-30-08 — LOW — audit-speedtree contradicts itself on the billboard clamp
`.claude/commands/audit-speedtree/SKILL.md:169-170` (Dim 3 checklist): "Billboard sizing
precedence … **every path clamped to `[16, 8192]`** (#1001/#1002)."
`.claude/commands/audit-speedtree/SKILL.md:243` (regression-guard list): "Billboard
extent clamping is `Option`-returning, **not** `f32::clamp` … a non-finite field falls
through to the next tier … Regression = a bare `clamp` reinstated on any tier" (#3529).

Both bullets are in the same file. The code
(`crates/spt/src/import/mod.rs:143-149`, `clamp_billboard_extent`) matches line 243:
`value.is_finite().then(|| value.abs().clamp(MIN…, MAX…))`. An auditor working the Dim 3
checklist top-down reads line 169 first, accepts a bare `f32::clamp` as conforming, and
misses precisely the NaN regression #3529 fixed. Fix: make line 169 defer to line 243.

### TD4-2026-08-30-09 — LOW — audit-speedtree calls a CLOSED issue "the one that remains open"
`.claude/commands/audit-speedtree/SKILL.md:105-109`: "Of the later SPT-NEW batch,
SPT-NEW-01 … and SPT-NEW-06 … are also **closed** — only SPT-NEW-07
(`MaybeStringElseBare` misparse risk on a bare tag-13005 immediately before the
geometry tail, **#1822**) remains open."

`gh issue view 1822` → **CLOSED**. Fixed by `19813460` ("Fix #3531 … reject a
zero-length 13005 candidate"). This is the skill's Phase-1 orientation step, so the
error lands before any dimension runs: the auditor starts with a false open-item list.

### TD4-2026-08-30-10 — LOW — audit-starfield points the truncation-tail check at two closed issues
`.claude/commands/audit-starfield/SKILL.md:291-292`: "The residual truncation tail in
Meshes01/MeshesPatch is tracked at **#746/#747** — confirm it has not grown."

Both CLOSED (#746 "SF-D1: Starfield shader-property tail-fields gated on `bsver == 155`
skip on BSVER 172"; #747 "SF-D1-DISPATCH: BSShaderType155 dispatch gated on
`bsver == 155`"). Neither is a truncation-tail tracker — they are the *version-gating*
defects whose fix reduced the tail. The live residual-truncation tracker is the
`bsweakreferencenode_2byte_gap` line of work (6/29,849 in MeshesPatch.ba2). An auditor
"confirming it has not grown" against two closed shader-gating issues learns nothing.

### TD4-2026-08-30-11 — LOW — audit-renderer's prescribed lockstep grep doesn't match its own expected answer
`.claude/commands/audit-renderer/SKILL.md:118`: "verify lockstep via
`grep -rl \"struct GpuInstance\" crates/renderer/shaders/` → `include/bindings.glsl`,
`triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp` (**5 declaration sites**)".

The command as written returns **6** files — it also matches
`crates/renderer/shaders/skin_vertices.comp`, whose hit is a *comment* at line 83–84
("this shader has no `struct GpuInstance` for the existing GpuInstance-lockstep tests
to anchor on"). The substantive claim (5 real declarations) is correct; the *recipe*
is not, and it guards the codebase's single highest-stated lockstep risk
(`feedback_shader_struct_sync.md`, severity floor HIGH). Fix: anchor the grep to a
declaration (`grep -rlE '^struct GpuInstance'`).

### TD4-2026-08-30-12 — LOW — `audit-tech-debt`'s own Dim 2 names the pre-#1044 canonical coord home, inverting the leak test

`.claude/commands/audit-tech-debt/SKILL.md` (Dimension 2, Z-up→Y-up bullet): "Z-up →
Y-up coordinate flips reimplemented outside the canonical homes
(`crates/nif/src/import/coord.rs`, `crates/nif/src/anim/coord.rs`) — **any other call
site is a leak**."

Since **#1044 / TD3-002** the single source of truth is
`crates/core/src/math/coord.rs` (`byroredux_core::math::coord::zup_to_yup_pos` /
`zup_to_yup_quat_wxyz`). Both named files say so themselves:
- `crates/nif/src/import/coord.rs:1-8` — "Array-form primitives live in
  [`byroredux_core::math::coord`]; this file wraps them with the NIF-internal types".
- `crates/nif/src/anim/coord.rs:4-14` — "Pre-#1044 / TD3-002 this file owned a divergent
  copy … **The single source of truth now lives in `byroredux_core::math::coord`**"; the
  file is now a 14-line `pub use` re-export.

An auditor applying the bullet as written flags all ~15 correct
`byroredux_core::math::coord::zup_to_yup_pos` call sites (`nif/import/mesh/tangent.rs`,
`sse_recon.rs`, `spt/import/mod.rs`, `nif/anim/{transform,keys,bspline}.rs`,
`core/animation/root_motion.rs`, …) as leaks — the exact inversion of the truth, on a
consolidation that is fully converged. Both paths resolve, so the gate passes.

Worth noting for its own sake: this is the audit skill *this audit runs from*, and it
is the twelfth confirmed instance of the same failure mode. The skill corpus is not
drifting in one or two places — it drifts wherever a claim outlives the code, and
nothing in the toolchain checks claims.

## Stale candidates — premise check, and one correction to my own triage (0 dropped)

- **audit-speedtree Dim 3, the 5-vs-8-float CNAM split.** I initially **dropped** this
  as a false alarm, because the *code* agrees with the skill:
  `crates/plugin/src/esm/records/tree.rs:160` states "CNAM is 5 × f32 on Oblivion, 8 ×
  f32 on FO3/FNV", and `crates/spt/src/import/mod.rs:78` repeats it. Skill and code
  agreeing is normally sufficient to close a doc-rot question from static analysis alone.

  **That reasoning was wrong, and I am recording it rather than quietly deleting it.**
  The concurrent `/audit-speedtree` run measured the real corpus:
  CNAM is **32 bytes / 8 floats on 142/142 Oblivion, 9/9 FO3 and 3/3 FNV** records
  (`docs/audits/AUDIT_SPEEDTREE_2026-08-30.md`). There is no 5-float Oblivion tier. So
  the claim is stale in the skill **and** in two production docstrings and a unit test —
  a drift I could not falsify from code, because the code carries the same bad premise
  the skill does.

  The methodological lesson, which matters more than the finding: static cross-checking
  detects a *disagreement* between doc and code; it cannot detect a claim both got wrong
  from the same source. Where a premise is about **on-disk data shape**, only a corpus
  measurement settles it. This audit is static by dispatch, so premises of that class
  are outside what it can close — it should say so rather than report "verified".

  Counted in the corpus tally below (attributed to `/audit-speedtree`, not re-filed here).

## Verified CLEAN (no drift)

- `_audit-validate.sh` path gate: 0 STALE across 2305 refs / 99 files.
- Dimension counts in all 19 dimension-bearing skills.
- GPU `#[repr(C)]` sizes across the whole corpus (160 / 368 / 432 B).
- `_audit-common.md` crate→owner map: covers all 25 live crates; only
  `crates/platform` (60 LOC placeholder) is unreferenced by any skill, and the file
  explicitly names it a placeholder needing no owner.
- `audit-suite --preset comprehensive` roster (25 entries) vs the live skill set:
  the 3 omissions (`publish`, `suite`, `incremental`) are meta-skills, intentional.
- `audit-safety` claims spot-checked: cxx-bridge is still a no-pointer placeholder
  (26 LOC, no `*mut`/`*const`); `MAX_REFRACT_PASSTHRUS` is still 8 and still the loop
  bound (`triangle.frag:1951,1994`).
- LOC figures in `_audit-common.md` (main.rs 1053, studio_host.rs 252, combat.rs 952,
  sdk 282, platform 60): all exact.
# Dimension 5: Stale Markers (TODO / FIXME / HACK / XXX)

**NO FINDINGS.** All 20 grep hits are documented false positives.

Discovery: `grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux` → 20 hits.
`grep -RInE '(TODO|HACK)' crates/renderer/shaders/` → **0 hits**.

Every hit triaged and dismissed:

| Class | Count | Sites |
|---|---|---|
| ESM `XXXX` extended-size protocol tag (skill-documented exclusion) | 14 | `esm/reader.rs` ×9, `esm/records/misc/magic.rs` ×3 (the `*b"XXXX"` wrong-type sentinel), `esm/cell/wrld.rs` ×1, `esm/reader.rs` test prose ×1 |
| Reference-implementation FIXME quoted as documentation of upstream | 3 | `crates/bgsm/src/bgem.rs:137` ("Order matches the reference's // FIXME note"); `esm/records/misc/world.rs:275` (OpenMW's own FIXME conceding its `worldspace == 0x3C` check is wrong); `crates/nif/src/blocks/bs_geometry.rs:596` (Bethesda `BSGeometryMeshData::Sync` line 1709) |
| Prose asserting the *absence* of a marker | 1 | `byroredux/src/groundcover_translate.rs:252` — "which is why the fallback lives in `GroundCoverPalette::resolve` rather than behind a `TODO` here" |
| Historical closure note | 1 | `byroredux/src/scene.rs:1482` — "Closes the #242 consumer-side TODO (#1055)" |
| Doc-comment prose (non-marker) | 1 | `esm/reader.rs:1321` regression-test doc |

**Zero live TODO/FIXME/HACK markers in the entire codebase** — production and shaders.
The third-party attribution block atop `crates/renderer/shaders/triangle.frag`
(GLSL-PathTracer MIT notice + Burley 2012 citation) is intact.

This is the cleanest this dimension has ever measured. The marker count is not
decaying — the codebase's convention of writing the deferral into a doc comment with
an issue number, instead of a bare marker, is holding.
# Dimension 6: Stub & Placeholder Implementations

**NO FILEABLE FINDINGS.** Two documented-deferral notes only.

- `grep -RInE 'unimplemented!|todo!\(\)|panic!\("not '` over `crates byroredux` → **0 hits**.
  The engine's explicit-fallback convention holds; there is no reachable panic stub.
- `grep -RInE '// *(stub|TODO: real|placeholder|not yet)'` → 49 hits, all read.
  Every one is either (a) a test fixture literal, (b) a doc comment *describing* an
  intentional fallback (`spt` placeholder billboard, NIF diffuse-slot placeholder,
  `MeshRegistry` unload placeholders), or (c) a Vulkan lifetime comment
  ("not yet destroyed", "not yet bound") — none is an unimplemented code path.
- `byroredux/src/commands/` → **0** commands print "not implemented" / "TODO" / no-op.

## Notes (not filed)

- **`crates/mod-runtime` has zero consumers** (1475 LOC, 1011 production; a workspace
  member, exported as `byroredux-mod-runtime` in `[workspace.dependencies]`). No file
  outside the crate references it. This is *documented* in `_audit-common.md`'s
  un-owned table ("Still has **no consumer in the engine**") and is the trust-boundary
  work landing ahead of its host. Per this dimension's own exclusion — "public API of a
  workspace-internal crate a future binary will consume (note such cases rather than
  deleting)" — noted, not filed. One byproduct *is* filed under Dim 8 (TD8-…-03).
- **`ImgsRecord.dnam_raw` is captured but never read** outside the parser
  (`esm/records/misc/world.rs:1255`). The doc comment states the deferral explicitly
  ("defers field-by-field decoding to the consumer. See #624 / SK-D6-NEW-03"), so it is
  a declared parser-side capture, not a silent stub. Its sibling `LgtmRecord` *is* now
  consumed (`byroredux/src/cell_loader/load.rs:633`, `lighting_from_template`).
# Dimension 7: Magic Numbers & Hardcoded Constants

## TD7-2026-08-30-01 — MEDIUM — `water.frag`'s RT reach budgets bypass the shader-constant pipeline, and one carries a `// matches triangle.frag` claim that is false

`crates/renderer/shaders/water.frag:194-196`:

```glsl
const float REFLECTION_MAX_DIST = 5000.0;
const float REFRACTION_MAX_DIST = 2000.0;
const float DIST_FALLOFF        = 0.0015; // matches triangle.frag
```

**None of the three exists in `crates/renderer/src/shader_constants_data.rs`** — the
documented single source that `shader_constants.rs` and `build.rs` both `include!` to
emit `shaders/include/shader_constants.glsl`. They are open-coded literals in a shader
that *does* `#include` that header for its other constants.

**The `// matches triangle.frag` comment is not true.** `0.0015` appears exactly once in
the entire shader tree — on this line. `triangle.frag` has no `DIST_FALLOFF` and no
`0.0015`; its nearest analogue is the glass optical-thickness `0.004`
(`triangle.frag:2309`), a different quantity. So the comment asserts a lockstep
relationship that (a) is unenforced and (b) does not currently hold.

The two `MAX_DIST` values *do* mirror `triangle.frag`, but by hand and by literal:
`5000.0` at `triangle.frag:2641`, and `2000.0` at `triangle.frag:1041`, `:1652` and
`:1966` (`const float REFRACT_MAX_REACH = 2000.0;`). `triangle.frag:1954` is candid
about it — "re-issued the query with a fresh **hard-coded** 2000.0 tMax". That is six
sites for two ray-reach budgets across two shaders with no shared definition.

**Why the existing gate does not catch this.** `crates/renderer/src/shader_constants.rs`
enforces provenance with a **per-shader named allowlist**, not a structural rule: it
asserts `!src.contains("const uint WATER_CALM")`, `!src.contains("const float
BLOOM_INTENSITY")`, `!src.contains("const float VOLUME_FAR")`,
`!src.contains("const uint THREADS_PER_CLUSTER")` and so on — each name someone
remembered to list. `water_frag_motion_enum_matches` guards five `WATER_*` names in this
very file and walks straight past the three constants three lines above them. The gate
can only catch redeclarations of *enumerated* names, so any newly introduced literal is
invisible to it by construction.

**Fix**, in order: (1) move all three into `shader_constants_data.rs` so both shaders
`#include` one definition — this makes the `// matches triangle.frag` intent real
instead of aspirational; (2) delete the now-redundant comment; (3) strengthen the gate
from a name allowlist toward a structural check — e.g. assert that no
`crates/renderer/shaders/*.{frag,vert,comp}` declares a top-level
`const float|uint|int <SCREAMING_NAME>` unless that name is present in
`shader_constants_data.rs`, with a small explicit exemption list for genuinely
shader-local values. Without (3) this dimension will keep re-finding new instances.
Severity: MEDIUM per the lockstep-drift floor (`feedback_shader_struct_sync.md`,
`_audit-severity.md` HIGH floor applies to `#[repr(C)]`/struct drift; these are scalar
budgets, so MEDIUM). Effort: small for (1)+(2), medium for (3).

## Checked, no finding

- **NIF version gating.** `grep -rInE 'version\(\) (>=|<=|==|>|<) 0x[0-9A-Fa-f]{8}'` over
  `crates/nif/src/blocks/` returns **0 hits** — every version gate goes through the named
  `NifVersion` predicate API. Raw `bsver()` comparisons: only 4 workspace-wide
  (`>= 34` ×2, `<= 34` ×1, `== 0` ×1). This is clean.
  *(Existing OPEN #3476 covers 19 raw version comparisons introduced by #2345 in a
  different parser — dedup, not re-filed here.)*
- **GPU `#[repr(C)]` size literals.** No inline size literal anywhere shadows
  `size_of::<GpuInstance/GpuCamera/GpuMaterial>()`; the three pinning tests in
  `gpu_instance_layout_tests.rs` / `material.rs` are the only assertions of those
  numbers, and every doc reference cites the test rather than restating a value.
- **Frame / ray / cache budgets** (`MAX_INSTANCES`, `MAX_TOTAL_BONES`, `MAX_MATERIALS`,
  `GLASS_RAY_BUDGET`, `MAX_REFRACT_PASSTHRUS`, `MIN_BLAS_BUDGET_BYTES`, …) are collected
  in `scene_buffer/constants.rs`, `acceleration/constants.rs` and
  `shader_constants_data.rs`. Spot-checked `MAX_REFRACT_PASSTHRUS`: still 8, still the
  loop bound (`triangle.frag:1951,1994`), and pinned by `shader_constants.rs:1416`.
- **ESM sub-record size literals.** No bare `if data.len() == N` size gates found outside
  the record structs that define them; the `XXXX` extended-size escape is handled once,
  in `esm/reader.rs:750-766`.
- **Protocol magic** (FourCC tags, BSA/BA2/NIF magic, Vulkan format enums) excluded per
  the skill.
# Dimension 8: Dead Code & Backwards-Compat Cruft

Baseline: 45 `#[allow(dead_code)]`, **0** `#[deprecated]`, **0** `// removed:` breadcrumbs.
All `_unused` locals (`esm/records/items.rs`, `script_instance.rs`, `cell/walkers.rs`,
`actor/mod.rs`, `nif/blocks/shader.rs`) are stream-position-advancing reads, not
refactor leftovers — correct, not debt.

## TD8-2026-08-30-01 — MEDIUM — 73 committed `_tmp_*` scratch examples, 6 978 LOC, in violation of the project's own documented convention

`git ls-files | grep -E 'examples/_tmp_'` → **73 files**: `crates/nif` 58,
`crates/sfmaterial` 10, `crates/plugin` 3, `crates/bsa` 1, `crates/facegen` 1.
That is **45 % of all 164 committed example targets** in the workspace.

These are one-shot audit probes. Their own headers say so — e.g.
`crates/nif/examples/_tmp_sk_d1_part.rs:1`:
`//! TEMP: validate remap_bs_tri_shape_bone_indices' single-partition identity shortcut.`
Twenty-three carry a literal `//! TEMP scratch` banner.

**The convention they violate is the project's own.** `docs/engine/exterior-readiness-plan.md:484`
documents the intended pattern: a scratch example is "`_tmp_land_stats.rs`, **deleted
after use**". None of these 73 was deleted. `.gitignore` has no `_tmp_` rule, so the
default outcome of an audit session is that its probes get committed.

**Amplification — this is why it clears the LOW default:** `cargo test` builds example
targets by default (to verify they compile) without running them. CI runs
`cargo test --workspace` (`.github/workflows/ci.yml:92`) plus a second
`cargo test --workspace` under the lock-order detector (line 120). So every CI run and
every local workspace test run compiles and links all 73 against `byroredux-nif`,
`byroredux-bsa`, `byroredux-plugin`, `byroredux-sfmaterial` and `byroredux-facegen` —
73 extra link steps, twice per CI run, for zero coverage. Meanwhile CI's clippy step is
`cargo clippy --workspace` *without* `--all-targets` (line 94), so these 6 978 lines are
built but never linted — they accrue lint debt invisibly.

**And it is still growing**: added 2026-08-03 (2), 08-07 (32), 08-08 (25), 08-12 (3),
08-16 (8), **08-29 (3 — yesterday)**.

**Existing issue**: #3150 (OPEN, `ESM-2026-08-20-D4-01`) covers exactly the **three**
`crates/plugin` probes. The other **70** are unfiled. Recommend re-scoping #3150 to the
whole set rather than filing a near-duplicate.

**Fix** (effort: small): `git rm` the 73 files, then add `crates/*/examples/_tmp_*` to
`.gitignore` so the convention enforces itself. Any probe worth keeping should be
promoted to a named, documented example or folded into a `#[ignore]`d corpus test.
Retention is cheap either way — they stay in git history.

## TD8-2026-08-30-02 — LOW — two dead `pub fn` NPC-spawn compatibility shims with 11 doc-comment references pointing at them

`byroredux/src/npc_spawn.rs:1080` `spawn_npc_entity` and `:1179`
`spawn_prebaked_npc_entity` are both `pub fn` carrying `#[allow(dead_code)]`, and both
have **zero call sites**. Every one of the 11 grep hits across `crates/core`,
`crates/plugin`, `byroredux/src/systems`, `save_io`, `cell_loader` and `scene.rs` is a
*doc or code comment* naming them, not a call.

Each is a ~30-line wrapper whose doc calls it a "synchronous compatibility entry point"
around the resumable job that superseded it
(`byroredux/src/npc_spawn/resumable.rs:38`, `NpcSpawnJob`):

```rust
let mut job = NpcSpawnJob::runtime(npc, race, game, ref_pos, ref_rot, ref_scale);
let mut budget = crate::cell_loader::FrameTimeBudget::unlimited();
match job.advance(...) {
    NpcSpawnProgress::Complete(result) => result.root,
    NpcSpawnProgress::Pending => unreachable!("an unlimited NPC spawn budget cannot yield"),
}
```

Per this dimension's rule — ByroRedux has no external consumers, so a
"for compatibility" entry point with no caller is pure rot. The amplifying detail is the
doc surface: `spawn_npc_entity` is one of the most-cited function names in the
codebase's prose (perks stamping, AI-package collapse #2031, save round-trip #1835,
idle phase-desync), all describing behaviour that now lives in `NpcSpawnJob`. A reader
following those references lands on a dead wrapper. Deleting the two shims forces those
11 comments to be re-pointed at the live code — which is the actual value here.
Effort: small.

## TD8-2026-08-30-03 — LOW — `byroredux-mod-runtime` is a dangling `[workspace.dependencies]` entry

`Cargo.toml:48` declares `byroredux-mod-runtime = { path = "crates/mod-runtime" }`, but
**no** member `Cargo.toml` contains `byroredux-mod-runtime = { workspace = true }`.
Swept every `[workspace.dependencies]` key against every member manifest; this is the
only genuine orphan (`env_logger` and `lz4_flex` came back as regex artefacts and are
consumed by 10 and 1 members respectively).

The crate itself (1 475 LOC) is a *deliberate* consumer-less landing, documented in
`_audit-common.md`'s un-owned table, so the crate is not the finding — the unused
workspace-dependency alias is. Either wire the alias where the host will consume it or
drop the line until then; a dangling alias makes `grep`-based consumer discovery report
a dependency edge that does not exist. Effort: trivial.

## Triaged and DISMISSED — all 45 `#[allow(dead_code)]` sites read

| Site(s) | Verdict |
|---|---|
| `core/ecs/query.rs:26,96,234` | RAII `RwLockReadGuard`/`WriteGuard` fields pinned so the cached `storage`/`component` pointer stays valid (#1367). Correct, documented. |
| `core/ecs/lock_tracker.rs:65` | `cfg_attr(not(debug_assertions))` — debug-only by construction. |
| `core/ecs/access.rs:255,267` | Test-struct payload fields. |
| `renderer/.../scene_buffer/buffers.rs:26` | `LightHeader.count` is GPU-write-only, byte-copied to the SSBO (TD2-203). Documented. |
| `hkx/packfile.rs:196` | `global_target` — the read half of the fixup table the parser validates anyway (#2267). Documented. |
| `nif/blocks/tri_shape/bs_tri_shape.rs:244,263,276` | `VF_UVS_2` / `VF_LAND_DATA` / `VF_INSTANCE` — schema-completeness constants held back under the no-guessing policy (#336/#358/#2578). Documented. |
| `byroredux/env_translate.rs:278` | `INHERIT_MAP` — parsed, deliberately unwired pending its first resolver. Documented. |
| `byroredux/components.rs:100,104` | `Locked.lock_level`/`key_form_id` — data plumbed ahead of the lockpicking system. Documented. |
| `byroredux/components.rs:1176,1382,1444,1448` + `cell_root_ref_index.rs`, `persistent_ref_index.rs` | Landed ahead of named pending consumers (EX-16 / #2372 / #3455 / stream-boundary continuity). Documented. |
| `plugin/src/legacy/mod.rs:35` (crate-wide) | ESM→`Record` scaffolding, `pub(crate)` per #1322, exercised by its own tests. Documented. |
| `groundcover_translate.rs:67,77,86,167,187` | All five carry "see `DEFAULT_AFFINITY` — Phase 1 scatter is the consumer". |
| `interaction.rs:162,601`, `asset_provider/material.rs:596,1025` | `cfg_attr(not(test), …)` — test-only by construction. |
| `bsa/src/ba2.rs:159,161` | **Existing #1761 (OPEN)**, and its premise re-verified: `start_mip` **is** read (`ba2.rs:683,688,692,697`, the chunk-monotonicity check) so its attribute is redundant; `end_mip` is set-never-read. Do not re-file. |
| test/example/fixture files | Out of scope. |

Not one unexplained `#[allow(dead_code)]` remains in the tree — every site either has an
inline rationale or is `cfg`-gated. That is a real improvement in this dimension.
# Dimension 9: Test Hygiene

Baseline: **169** `#[ignore]` sites (`^\s*#\[ignore` over `.rs` under `crates`/`byroredux`,
the #3456-widened pattern that catches the `#[ignore = "..."]` reason form).

## TD9-2026-08-30-01 — LOW — 80 % of `#[ignore]`s carry no machine-readable reason, and that has already produced one wrong audit baseline

Of 169 sites, **33 (20 %)** use the documented reason form `#[ignore = "…"]`; **136
(80 %)** are bare `#[ignore]` with the gate condition stated only in an adjacent
`///` doc comment.

Triaged all 169: **every one is legitimately gated** — on-disk game corpora, a Vulkan
device, an audio device, a release build, or a one-shot calibration bench. There is no
`#[ignore]` in this codebase hiding a broken test. The 14 distinct reason strings in use
are all of the form "requires FNV BSA — opt in with `--ignored`" / "requires an
RT-capable Vulkan device and a display/Xvfb" / "needs Skyrim SE game data on disk;
~1 GB resident".

The finding is the **inconsistency**, not any individual test. The reason string is the
only form a tool can read; the doc comment is not. That gap has already cost this audit
suite once: **#3440** (OPEN, `TD4-2026-08-27-03`) records that
`AUDIT_TECH_DEBT_2026-08-24.md` published an `#[ignore]` baseline of 171 where the real
figure was 121 — and **#3456** had to widen this dimension's own discovery regex after
the bare-`]` pattern silently dropped every reason-form test (a 19 % undercount at the
time). Both are symptoms of the same thing: the population is not uniformly
self-describing, so every count of it is a judgement call.

**Fix**: convert the 136 bare sites to `#[ignore = "<existing doc-comment reason>"]` —
purely mechanical, the reason text already exists one line above in nearly every case.
Then this dimension's triage becomes `grep -oE 'ignore = "[^"]*"' | sort | uniq -c`
instead of reading 169 doc comments, and a future `#[ignore]` with no reason becomes a
reviewable anomaly. Effort: small. Kind label: `test-gap`.

## Checked, no finding

- **`#[ignore]`s guarding a closed CRITICAL/HIGH fix** (the MEDIUM promotion trigger):
  cross-referenced every backticked identifier in the audit skill corpus (815 symbols)
  against `#[ignore]`d test functions. **10 matches**, each read individually — all are
  corpus/device gates, none is a regression guard disabled for an unrelated reason:
  `parse_rate_fallout_4`, `parse_rate_starfield`, `parse_rate_fo4_all_meshes`,
  `cross_game_translation_completeness`, `parse_real_skyrim_esm`,
  `race_oblivion_data_and_subs_against_vanilla`, `clas_oblivion_knight_against_vanilla`,
  `da10_pex_reproduces_hand_builder_byte_for_byte`, and the two audio emitter-prune
  guards (`looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`,
  `non_looping_emitter_stops_on_emitter_remove_regression_858`). The audio pair names a
  closed issue (#858) but is gated on "working audio device + vanilla FNV data" — a real
  hardware gate, correctly applied.
- **Commented-out assertions inside passing tests**: `grep -RInE '^\s*// *assert'` →
  **0 hits** after excluding prose that quotes an assertion for explanation. Clean.
- **Vacuous / smoke-only tests**: swept every `#[test]` body for "no assertion at all"
  and "only `assert!(x.is_ok())`". 27 candidates survived the first filter; all 27 read
  and dismissed — they either delegate to an assert-carrying helper
  (`crates/ui/src/catalog.rs`, `crates/nif/src/blocks/interpolator_tests.rs`,
  `crates/nif/src/import/material/texture_slot_3_4_5_tests.rs` → `assert_path`), are
  `#[should_panic]`, or are deliberate "does not panic / is a no-op" tests where absence
  of a panic *is* the assertion (`crates/audio/src/tests.rs:11`,
  `crates/scripting/src/timer.rs:172`, `byroredux/src/systems/audio.rs:760`). No test in
  this workspace asserts nothing by accident.
- **`byroredux/tests/golden_frames.rs`**: present, `#[ignore = "requires Vulkan device +
  release build; opt-in via --ignored"]`, with a documented regeneration path
  (`BYROREDUX_REGEN_GOLDEN=1`). Still runnable as documented.
- **`#[cfg(feature = "…")]`-gated tests never enabled in CI**: the two feature-gated test
  invocations (`--features dhat-heap` on `heap_allocation_bounds` /
  `heap_allocation_bounds_geometry`) *are* explicitly run by
  `.github/workflows/ci.yml:184-185`. No orphan feature gate found.

## Adjacent observation (filed under Dim 8, not here)

CI's clippy step is `cargo clippy --workspace -- -D warnings` **without `--all-targets`**
(`.github/workflows/ci.yml:94`), while `cargo test --workspace` (lines 92 and 120) *does*
build every example target. So the workspace's 164 example targets — 73 of them committed
`_tmp_*` audit scratch — are compiled twice per CI run and linted zero times. See
TD8-2026-08-30-01.

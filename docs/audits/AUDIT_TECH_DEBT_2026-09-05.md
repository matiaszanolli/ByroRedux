# Tech-Debt Audit — 2026-09-05

**Scope**: all 9 dimensions, `--depth deep`. Nine dimension agents, each report
verified against the live tree before merge.
**Commit at audit time**: `fa5c4191` (branch `main`).
**Dedup baseline**: 500 `tech-debt`-labelled issues (12 open) + the five prior
`AUDIT_TECH_DEBT_*` reports.

## Executive Summary

**54 findings — 0 CRITICAL, 0 HIGH, 8 MEDIUM, 46 LOW.**
Effort: 29 trivial · 14 small · 10 medium · 1 large.

The dominant theme this sweep is not decay in the engine — it is **decay in the
machinery built to detect decay**. Five of the eight MEDIUMs, and a large share
of the LOWs, are drift-detectors that have themselves drifted:

- `bindings.glsl` pins `GpuMaterial` to a test name that **has never existed**
  anywhere in the workspace (TD3-01).
- The shader-constant provenance gate added two days ago cannot see
  `shaders/include/`, function-local `const`s, or `#define`s — i.e. it is blind
  to the file in the previous bullet (TD7-02).
- `_audit-common.md` — the file every audit reads first — sizes `crates/sdk` at
  282 LOC against a live 14,050 (TD4-01), and demonstrably misled
  `/audit-scripting` inside the 90-day window.
- The only pixel-level render regression guard cannot pass: its baseline
  predates 550 renderer commits and the switch to FSR3-by-default (TD9-01).
- ≥101 of 182 data-gated tests report green `ok` when their corpus is absent
  (TD9-02).

**Two findings converge on one blind spot** and should be fixed together:
TD3-01's stale `GpuMaterial` comment lives in `bindings.glsl`, precisely the
file TD7-02's provenance gate structurally cannot scan — while
`feedback_shader_struct_sync.md` names that file the #1 source of silent
GPU-struct desync. Fixing either alone leaves the hole open.

**The engine code itself is in good shape.** `unimplemented!`/`todo!()` remains
at **0**; live TODO/FIXME/HACK markers remain at **0** for the fourth
consecutive sweep; there are zero `#[deprecated]` items, zero `// removed:`
breadcrumbs, zero commented-out assertions, and zero single-branch feature
flags. Dimension 1's growth is real but concentrated in young crates that have
never had a sweep.

### Delta vs the 2026-08-30 baseline

| Metric | 2026-08-30 | 2026-09-05 | Note |
|---|---|---|---|
| Files >2000 **production** LOC | 5 | **12** | +7; four are young-crate first-sweep entries |
| TODO/FIXME/HACK/XXX (live) | 0 | **0** | all 20 hits are documented exclusions |
| `unimplemented!` / `todo!()` | 0 | **0** | codebase prefers explicit fallbacks |
| `#[ignore]` (bare form) | 0 | **0** | #3749's reason-string conversion holding |
| `allow(dead_code)` | — | 43 | 2 proven stale (TD8-04/05) |

`context/draw.rs` **left** the oversized bucket (~3620 → ~1760 production) via
#3282 — recorded so it is not re-proposed.

## Baseline Snapshot

Re-runnable counts, for the next audit to diff against:

```
TODO/FIXME/HACK/XXX:     20   (all documented exclusions; 0 live)
allow(dead_code):        43
unimplemented!/todo!():   0
#[ignore] tests:        181   (crates+byroredux; 182 tree-wide)
#[ignore] BARE form:      0
production LOC >2000:    12
total LOC >2000:         40   (secondary bucket)
open tech-debt issues:   12
```

Primary bucket (production LOC), highest first: `extensions.rs` 5921 ·
`sdk/compatibility.rs` 3759 · `scripting/papyrus_provider.rs` 3711 ·
`mod-runtime/runtime.rs` 3495 · `volumetrics.rs` 2937 · `context/mod.rs` 2645 ·
`scripting/fragment.rs` 2538 · `boot.rs` 2232 · `mesh.rs` 2230 ·
`nif/import/walk/mod.rs` 2165 · `texture_registry.rs` 2063 ·
`asset_provider/material.rs` 2044.

Four are already tracked (#3737 · #3736 · #3451 · #2256) and were referenced,
not re-filed.

## Top 10 Quick Wins

All trivial effort; the first two are MEDIUM severity.

| # | ID | Fix |
|---|----|-----|
| 1 | `TD3-2026-09-05-01` | `bindings.glsl`: 396 B → 432 B, and point at the real `gpu_material_size_is_432_bytes` |
| 2 | `TD4-2026-09-05-01` | `_audit-common.md`: correct the `crates/sdk` row (282 LOC → ~14,050, 2 files → 25) |
| 3 | `TD3-2026-09-05-02` | Four doc sites still say `compute_blas_budget` / `VRAM / 3` after `fa5c4191`; one is a broken intra-doc link |
| 4 | `TD4-2026-09-05-03` | Same rename, two audit SKILL files |
| 5 | `TD3-2026-09-05-06` | `CLAUDE.md`: "162 tests" → 746 (loaded every session, and duplicated state its own policy forbids) |
| 6 | `TD8-2026-09-05-04` | Delete stale `#[allow(dead_code)]` on `SkyParamsRes::texture_indices` — it has a production caller |
| 7 | `TD8-2026-09-05-05` | Same for `ActionState::was_released` — `extensions.rs` calls it |
| 8 | `TD3-2026-09-05-03` | CTDA function count: docs say 13 / ~15, `ConditionFunction::CATALOG` holds 19 |
| 9 | `TD8-2026-09-05-07` | Drop 3 unused dependencies (5th recurrence — consider a CI gate instead) |
| 10 | `TD7-2026-09-05-05` | `parse_weather_data`: use the `SKYRIM_DATA_SIZE` constant sitting six lines above it |

## Top 5 Medium Investments

| # | ID | Investment |
|---|----|-----------|
| 1 | `TD1-2026-09-05-01` | Split `extensions.rs` (5921 prod LOC, +8.5k lines in 5 days). Start with the ~1040-LOC legacy-extender block — it is the sole consumer of 44 `PAPYRUS_STORAGE_UTIL_*_ROUTE` imports. |
| 2 | `TD9-2026-09-05-02` | Give the Rust data-gated tests the strict mode the shell smoke gates already have (`exit 77` → `::error::`, #3003). ≥101 tests currently pass by doing nothing. |
| 3 | `TD7-2026-09-05-02` | Widen #3815's provenance gate to `shaders/include/`, `#define`s and function-local `const`s — pair with quick win #1, they are the same hole. |
| 4 | `TD1-2026-09-05-02/03/04` | First split of the three young crates. Note the SKILL's proposed `ExtenderFamily` axis for `compatibility.rs` was **disproven** (30 of 3759 lines); split by service surface — StorageUtil alone is 55%. |
| 5 | `TD2-2026-09-05-03` | A `GpuImage` analogue to the consolidated `GpuBuffer`: 12 hand-rolled create→allocate→bind→view chains, whose absence has produced four separate issues (#1163/#1164/#1165/#2178) each fixing the same defect in a different copy. |

## Verification Notes (orchestrator)

Every MEDIUM was re-verified against the tree before merge, plus a sample of
LOWs. Three corrections were applied to what the dimension agents reported:

1. **TD5-01's impact was overstated.** `tools/` holds 112 marker hits, but
   **all 112 are in `tools/nifskope`**, a vendored third-party viewer that
   `_audit-common.md` explicitly excludes from first-party auditing. The four
   first-party tools have **zero** markers. The recipe gap is real; nothing was
   actually missed, and a naive "add `tools/`" fix would import 112 vendored
   hits. Any fix must exclude `nifskope`.
2. **TD5-01 and TD9-03 are one defect, reported twice** — the audit recipes
   (and the Phase-1 baseline) do not scan `tools/`. Kept separate because they
   name different recipes, but they should be fixed in one edit, subject to
   correction (1).
3. **TD6/TD8/TD9's shared "orphan branch" lead resolved three different ways**,
   and each was checked individually rather than treated as a block:
   #3170 is genuinely unfixed on main (verified: `RulesetBuilder` has no
   Skyrim/Oblivion arm, and `bbd501a1` is on no branch reachable from `main`);
   #2266's defect **was** fixed on main by a different commit (`211a23cc`,
   "Fix #3747") and was correctly not re-filed; #3084 is fixed on main by
   `bdc0d84e`. "Closed ≠ merged" held for the branch, but not for the defects.

Independently confirmed: `extensions.rs` created 2026-08-31 (`24df5304`), 66
commits, 10,652 lines · `gpu_material_size_is_396_bytes` occurs exactly once in
the workspace (inside the stale comment itself) and the GLSL struct **body** is
in sync, so TD3-01 is doc rot, not a #3829-class layout bug ·
`renderer_shader_sources` is non-recursive and filters to `.frag|.vert|.comp` ·
golden-frame baseline last touched 2026-06-04 with 550 renderer / 272 shader
commits since · the FormId root-index modules reference only each other.

## Deferred

None gated on an in-progress milestone. Two findings name blockers that have
since shipped and are therefore *unblocked*, not deferred: `TD6-2026-09-05-02`
(`stealth.rs` cites #446, closed, and M42 shipped seven procedure runtimes) and
`TD8-2026-09-05-01` (both #2369 and #2372 closed 2026-08-31 without wiring the
index the allows were protecting).

---

## Findings by Severity

## Finding Index

| ID | Sev | Effort | Title |
|----|-----|--------|-------|
| `TD1-2026-09-05-01` | MEDIUM | large | `extensions.rs` is the largest production file in the workspace — a 28-field / ~60-method `ExtensionHost` God Object built in five days |
| `TD2-2026-09-05-01` | MEDIUM | small | the GENERAL-layout accumulator clear sandwich exists in four copies, and #3646/#3647 plus its pin test enumerate only three |
| `TD2-2026-09-05-02` | MEDIUM | small | `parse_skyrim_shader_base` is the shared Skyrim+ shader head, but its two inline twins got #2603's gap-band predicates and it did not |
| `TD3-2026-09-05-01` | MEDIUM | trivial | `bindings.glsl` documents `GpuMaterial` as 396 B and points the struct-sync invariant at `gpu_material_size_is_396_bytes` — a test that has never existed (live: 432 B / `_432_`) |
| `TD4-2026-09-05-01` | MEDIUM | trivial | `_audit-common.md`'s `crates/sdk` layout row understates the crate ~50× — 282 LOC / 2 files against a live 14,050 LOC / 25 files, in an un-owned crate |
| `TD6-2026-09-05-01` | MEDIUM | small | `skyrim_ruleset` / `oblivion_ruleset` are production-unreachable — `build_ruleset` silently returns `None`, and #3170's landed fix never reached `main` |
| `TD9-2026-09-05-01` | MEDIUM | small | The only pixel-level render regression guard cannot pass — its baseline predates FSR3 becoming the default upscaler *and* predates the bench mode it now invokes |
| `TD9-2026-09-05-02` | MEDIUM | medium | At least 101 of 182 `#[ignore]`d real-data tests report a green `ok` when their data is absent — the Rust half of the tree has no skip signal, while the shell half already does |
| `TD1-2026-09-05-02` | LOW | medium | `compatibility.rs` is 3759 production LOC and 55 % StorageUtil — and the SKILL's proposed `ExtenderFamily` split axis does not exist in the code |
| `TD1-2026-09-05-03` | LOW | medium | `papyrus_provider.rs` is a compiler front-end, an IR, and an interpreter in one 3711-LOC file |
| `TD1-2026-09-05-04` | LOW | medium | `mod-runtime/runtime.rs` holds 19 separate `impl <wit>::Host for HostState` blocks in one 3495-LOC file (the SKILL's per-binding axis is CORRECT — verified) |
| `TD1-2026-09-05-05` | LOW | medium | `fragment.rs` is 2538 production LOC of 2540 total, with a 519-LOC `apply_effect` and 18 near-identical `populate_*` entry points |
| `TD1-2026-09-05-06` | LOW | small | `boot.rs` crossed 2232 production LOC — promote #3739's five `register_*_systems` functions to five files |
| `TD1-2026-09-05-07` | LOW | small | `walk/mod.rs` crossed 2165 production LOC — split the three independent satellite walkers out (note: the SKILL's stated rationale, "per the module doc's own category list", does not exist) |
| `TD1-2026-09-05-08` | LOW | medium | `asset_provider/material.rs` crossed 2044 production LOC because `merge_external_material` grew 37 % to 931 LOC since #2412 assessed it at 678 and recommended awareness only |
| `TD1-2026-09-05-09` | LOW | medium | the #2731 / #3282 file splits produced six single-function files — the extracted functions were relocated, never decomposed |
| `TD1-2026-09-05-10` | LOW | trivial | `storage_util_form_type_id` is a 105-arm FourCC→i32 match that should be a static table |
| `TD1-2026-09-05-11` | LOW | trivial | #2256 escalation — `volumetrics.rs` is now 2937 production LOC and `new_inner` is 774, not the 556 the issue records |
| `TD2-2026-09-05-03` | LOW | medium | twelve hand-rolled "create image → allocate → bind → view" chains, while the buffer side of the same problem has been consolidated for a year |
| `TD2-2026-09-05-04` | LOW | small | `ImageSpaceModifierFrame` and `ImageSpaceModifierView` are the same 14-field struct in two crates, joined by a hand-written field-by-field copy |
| `TD2-2026-09-05-05` | LOW | trivial | `FloatTarget` / `ColorTarget` are duplicated verbatim across the `nif` → `core` boundary, bridged by a 20-arm identity match |
| `TD2-2026-09-05-06` | LOW | medium | `extensions.rs` repeats the guest-entry snapshot prologue ten times and the 11-field `DeliveryCommitContext` literal fourteen times |
| `TD2-2026-09-05-07` | LOW | small | 111 hand-written full-field `NifHeader` literals across 40 files, with ~18 rival local factory functions, while `NifHeader::detached` produces exactly that value |
| `TD2-2026-09-05-08` | LOW | small | the `SubRecord` test-fixture builder is defined 32 times across `crates/plugin`, under three names and three incompatible signatures |
| `TD3-2026-09-05-02` | LOW | trivial | today's `fa5c4191` renamed `compute_blas_budget` → `probe_blas_heap_bytes` and changed its formula; four doc sites still describe the old name and the old `heap / 3` math |
| `TD3-2026-09-05-03` | LOW | trivial | `docs/feature-matrix.md` says the CTDA evaluator covers "13 functions" and `npc-spawn-ai-packages.md` says "~15"; the live `ConditionFunction::CATALOG` holds 19 |
| `TD3-2026-09-05-04` | LOW | trivial | `triangle.frag` describes R1 Phase 6 as still pending in three present-tense comments, and attributes the UV/alpha identity defaults to `GpuInstance::default()` — a struct that has carried none of those fields since 2026-05-01 |
| `TD3-2026-09-05-05` | LOW | trivial | `legacy_pbr_translation_tests.rs`'s module doc still names the deleted `Material::classify_pbr` as a live sharing partner — the site #1624's own SIBLING completeness check was meant to catch |
| `TD3-2026-09-05-06` | LOW | trivial | `CLAUDE.md`'s Quick Reference says `cargo test -p byroredux-core` runs "162 tests"; the crate carries 746 `#[test]` functions |
| `TD4-2026-09-05-02` | LOW | trivial | six more LOC figures in `_audit-common.md`'s Binary / Gameplay rows are stale — one by 98%, one contradicted by this audit's own skill file |
| `TD4-2026-09-05-03` | LOW | trivial | two audit SKILL files still name `compute_blas_budget`, renamed hours ago by `fa5c4191`; one pins a stale line anchor, the other states the pre-#3839 formula |
| `TD4-2026-09-05-04` | LOW | trivial | ~13 backtick-convention violations in the docs advisory — deliberately-absent, forward-looking and deleted names asserted as existing, one of them self-contradictory |
| `TD4-2026-09-05-05` | LOW | trivial | seven dead backticked bare basenames sit in the skill tier itself, invisible to the gate per #3439 — and one of them makes `audit-incremental` state a fact that is wrong for three of its four names |
| `TD4-2026-09-05-06` | LOW | small | 26 CRITICAL/HIGH findings across 12 pre-`/audit-publish` reports have no GitHub trace — the mandated `docs/audits/` dedup step returns false-NEW on already-fixed work |
| `TD5-2026-09-05-01` | LOW | trivial | Dim 5's discovery recipe never looks at `tools/` — 4 first-party workspace crates, 4 706 LOC, invisible to four consecutive audits |
| `TD5-2026-09-05-02` | LOW | trivial | the two Dim 5 grep patterns disagree with each other, and neither sees the `TBD` convention the codebase actually uses |
| `TD6-2026-09-05-02` | LOW | trivial | `crates/core/src/stealth.rs` justifies its zero-consumer state with a blocker that shipped — #446 is closed and M42 delivered seven procedure runtimes |
| `TD7-2026-09-05-01` | LOW | small | Five of the six `GpuRayBudget` policy ceilings are hand-retyped as shader loop bounds — only `glass_ray_limit` was ever derived |
| `TD7-2026-09-05-02` | LOW | medium | #3815's shader-constant provenance gate is blind to function-local `const`s, every `#define`, and the whole `shaders/include/` tree |
| `TD7-2026-09-05-03` | LOW | trivial | The mesh-ID no-history bit and its complement mask are hand-typed at five GLSL sites — including the shader that *writes* the bit — and have no Rust-side constant at all |
| `TD7-2026-09-05-04` | LOW | trivial | `shader_constants_data.rs` hand-copies `MAX_BONES_PER_MESH = 144` where its own re-export pattern and an existing build-dependency allow a derivation |
| `TD7-2026-09-05-05` | LOW | trivial | `parse_weather_data` decodes the WTHR DATA payload with bare offsets while the named `SKYRIM_DATA_SIZE` sits six lines above it |
| `TD8-2026-09-05-01` | LOW | small | The whole FormId→Entity single-root index subsystem is dead, and both milestones its `#[allow(dead_code)]`s name as gates closed on 2026-08-31 |
| `TD8-2026-09-05-02` | LOW | trivial | `load_interior_cell` is a dead `pub fn` behind a dead re-export — the same synchronous-superseded-by-resumable-job pattern as #2266/#3747, one file over |
| `TD8-2026-09-05-03` | LOW | small | `crates/core/src/animation/controller.rs` (454 LOC) is a fully dead subsystem — nothing constructs `AnimationController` outside its own tests, and no system reads it |
| `TD8-2026-09-05-04` | LOW | trivial | `SkyParamsRes::texture_indices`'s `#[allow(dead_code)]` is stale — it has a production caller, and the 5-line justification above it is false |
| `TD8-2026-09-05-05` | LOW | trivial | `ActionState::was_released`'s `#[cfg_attr(not(test), allow(dead_code))]` is stale — `extensions.rs` calls it in production |
| `TD8-2026-09-05-06` | LOW | trivial | `MaterialProvider::register_starfield_cdb` is a test-only duplicate of the shipped CDB registration path, and its doc names a production caller that calls a different method |
| `TD8-2026-09-05-07` | LOW | trivial | Three unused dependencies across three manifests — a fresh crop of the #2426–#2431 / #2075 class |
| `TD8-2026-09-05-08` | LOW | small | Seven production `#[allow(unused_imports)]` on `cell_loader.rs`'s re-export blocks suppress the compiler's only dead-re-export detector — in a binary crate that has no external API surface to protect |
| `TD8-2026-09-05-09` | LOW | trivial | `QuestStageState`'s four dynamic-subscription methods are superseded by three static subscriber-ID constants and survive only on their own unit tests |
| `TD9-2026-09-05-03` | LOW | trivial | Dim 9's own discovery recipe — and the Phase-1 baseline snapshot — are structurally blind to `tools/` |
| `TD9-2026-09-05-04` | LOW | trivial | Five `recon`-gated `crates/spt` example binaries have no compile gate in any lane |
| `TD9-2026-09-05-05` | LOW | trivial | `cargo test -p byroredux-core` — the command CLAUDE.md documents for core tests — silently drops the two `inspect`-gated round-trip tests that only the workspace lane compiles |

## MEDIUM

### TD1-2026-09-05-01: `extensions.rs` is the largest production file in the workspace — a 28-field / ~60-method `ExtensionHost` God Object built in five days


- **Severity**: MEDIUM
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/extensions.rs` (5921 production / 10652 total LOC); `ExtensionHost` struct at `:329`, its single `impl` block `:360`–`:3539`
- **Status**: NEW
- **Age**: created `24df5304`, 2026-08-31 ("feat(engine): host sandboxed extensions natively") — 2144 total LOC at birth, **10652 today across 66 commits in 5 days**
- **Description**: One file carries 2.96× the split threshold and is 1.6× the next-largest
  production file. `impl ExtensionHost` is a single 3180-line block holding ~60 methods over
  six responsibilities that share no state beyond the struct itself. This is the same
  architectural shape #3736 was promoted to MEDIUM for (`VulkanContext`, 128 fields) — CLAUDE.md
  Architecture Invariant 1, "ECS over scene graph … No God Objects".
- **Evidence**: the six responsibilities are physically contiguous and separable —

  | Region (symbols) | ≈LOC | Responsibility |
  |---|---|---|
  | imports `:8`–`:117` + `EXTENSION_STATE_RESOURCE`…`MAX_PENDING_REPUTATION_WRITES`, `EntityHandleRegistry`, `HostedComponent`, `HostedConsoleCommand`, `HostedScriptFunction`, `RecurringCadence` | 360 | types + 110 lines of `use`, 44 of which are `PAPYRUS_STORAGE_UTIL_*_ROUTE` constants |
  | `ExtensionHost::new` → `install_package` → `console_commands` / `invoke_console_command` / `papyrus_provider_catalog` / `invoke_papyrus_provider` / `invoke_owned_papyrus_provider` / `enqueue_published_event` / `invoke_mod_event` | ~690 | lifecycle + host-service dispatch |
  | `invoke_storage_util`, `invoke_storage_util_prefix`, `invoke_storage_util_form_filter`, `invoke_storage_util_list`, `invoke_legacy_container` | **~1040** | PapyrusUtil / JContainers legacy-extender shims |
  | `bind_entity`…`dispatch_updates` (8 public `dispatch_*` + their `_with_projections` / `_inner` twins) | ~800 | canonical event delivery |
  | `validate_saved_state`, `capture_saved_state`, `restore_saved_state`, `decode_saved_state` | ~420 | persistence |
  | `apply_delivery_result`, `Resolved{ActorValueWrite,PlayIdle,ReputationWrite}`, `DeliveryCommitContext`, `take_resolved_*`, `apply_pending_{actor_value_writes,package_evaluations,animation_commands,reputation_writes,world_commands}` | ~600 | command write-back |
  | `entity_projection`, `RawEntityProjection`, `capture_spatial_snapshot`, `capture_entity_projections`, `capture_package_form`, `capture_package_candidates`, `forms_by_entity`, `entities_by_form` | ~640 | ECS → SDK snapshot capture |
  | `extension_{activation,cell_load,equipment,input,session,hit,update}_dispatch_system`, `emit_diagnostics` | ~430 | the seven scheduler-registered ECS systems |
  | `ExtensionHostSlot`, `ExtensionConsoleCommand`, `SessionEventQueue`, `sync_extension_script_function_invoker`, `engine_settings_snapshot`, `settings_snapshot_from_registry`, `register_extension_setting` | ~430 | ECS resources + settings registration |

  Six production functions here exceed 200 LOC: `invoke_storage_util_list` (388),
  `capture_entity_projections` (383), `invoke_storage_util` (259), `invoke_legacy_container` (256),
  `apply_delivery_result` (221), `install_package` (220).
- **Impact**: every extension-host change — a new event kind, a new legacy shim, a new snapshot
  field — recompiles and re-reviews a 10.6k-line translation unit, and every one of the 66 commits
  so far has landed in it. The split cost is growing roughly 1.7k lines/day; deferring is not
  neutral. Blast radius is contained to the binary (nothing outside `byroredux/` imports it).
- **Related**: #3736 (same God-Object class, `VulkanContext`); TD1-…-02 (`crates/sdk/src/compatibility.rs`
  is this file's declaration-side twin and carries the same StorageUtil bulk); the crate is listed
  as an un-owned subsystem in `.claude/commands/_audit-common.md`.
- **Suggested Fix**: promote the table above to a directory — `extensions/{mod,install,legacy_compat,dispatch,persist,commands,capture,systems}.rs`. Take
  `legacy_compat.rs` first: it is the largest single block (~1040 LOC), it is the only region that
  needs the 44 `PAPYRUS_STORAGE_UTIL_*_ROUTE` imports, and moving it deletes ~40 lines from the
  `use` wall at the top of every other region. `ExtensionHost` stays one struct — this is a file
  split, not a state split — so no ordering or guard-drop invariant is touched.
- **Effort**: large (decompose first: land `legacy_compat.rs` alone as step 1)

---

---

### TD2-2026-09-05-01: the GENERAL-layout accumulator clear sandwich exists in four copies, and #3646/#3647 plus its pin test enumerate only three


- **Severity**: MEDIUM (promotion trigger: duplicated logic with divergent bug-fix history — one copy got a fix the others did)
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - `crates/renderer/src/vulkan/caustic.rs` — `CausticPipeline::dispatch` (moving-camera `else` branch, `pre_clear_barrier`/`post_clear_barrier`)
  - `crates/renderer/src/vulkan/caustic.rs` — `CausticPipeline::clear_for_skip`
  - `crates/renderer/src/vulkan/volumetrics.rs` — `VolumetricsPipeline::record_neutral_frame` (`to_clear`/`to_sample`)
  - `crates/renderer/src/vulkan/water_caustic.rs` — `WaterCausticAccum::clear_pre_render_pass` (`pre_clear`/`post_clear`) ← the copy that was not enumerated
- **Status**: NEW
- **Description**: Four sites implement byte-for-byte the same contract —
  *barrier GENERAL→GENERAL into `TRANSFER_WRITE`; `vkCmdClearColorImage` with
  `uint32: [0,0,0,0]`; barrier back out of `TRANSFER_WRITE` to
  `SHADER_READ|SHADER_WRITE`* — on a per-FIF `R32_UINT` accumulator. The rule
  that makes the sandwich correct (the source scope must name `TRANSFER` so a
  *prior visit's* clear on the same slot chains into this one) was worked out
  once under #3646/#3647 and applied to three of the four. There is no shared
  helper, so the fourth copy simply was not in the author's field of view — and
  the source-shape guard test written to stop exactly this drift enumerates the
  same three files by name.
- **Evidence**:
  - `1889585a` (2026-08-30, "Fix #3646: carry skip-path clears into the next slot visit's barrier scope") touches **`caustic.rs` and `volumetrics.rs` only** (`git show --stat 1889585a`). `water_caustic.rs` was last touched by `c2336ee1` (2026-07-21), five weeks earlier.
  - After that commit, the three fixed sites read
    `.src_access_mask(SHADER_READ | SHADER_WRITE | TRANSFER_WRITE)` with
    `PipelineStageFlags::COMPUTE_SHADER | FRAGMENT_SHADER | TRANSFER` in the
    source stage. `WaterCausticAccum::clear_pre_render_pass` still reads
    `.src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)`
    with `vk::PipelineStageFlags::FRAGMENT_SHADER` — no `TRANSFER` on either axis.
  - The pin test `mod skip_clear_mask_pin_tests` (`crates/renderer/src/vulkan/caustic.rs`)
    has exactly two arms — `caustic_skip_clear_and_next_visit_agree_on_transfer`
    (`include_str!("caustic.rs")`) and
    `volumetrics_neutral_clear_and_next_visit_agree_on_transfer`
    (`include_str!("volumetrics.rs")`). There is no `water_caustic.rs` arm, so
    the fourth copy is unguarded as well as unfixed.
  - `clear_pre_render_pass` runs unconditionally every frame the accumulator
    exists (`crates/renderer/src/vulkan/context/build_and_upload_instances.rs`,
    inside `if let Some(ref wca) = self.water_caustic_accum`), so the
    prior-visit-was-a-clear case is the *normal* shape here, not an edge case.
- **Impact**: Maintenance, primarily: a fifth accumulator (or a fifth revision
  of the rule) has four places to land instead of one, and the guard test does
  not scale with the copies. **Correctness rider, explicitly unverified:**
  whether the missing `TRANSFER` in `clear_pre_render_pass`'s source scope is a
  live WAW hole depends on whether a frame can skip *both* `water.frag`'s
  atomics and `composite.frag`'s `texelFetch(waterCausticTex, …)` — that
  `texelFetch` sits behind a `params.caustic_flags.x > 0.5` select. I did not
  settle that; it is a synchronisation question and belongs to
  `/audit-renderer`, not to this dimension. The duplication and the
  three-of-four divergence are proven independently of how it resolves.
- **Related**: #3646, #3647 (both CLOSED by `1889585a`); #653 (the "mask must be
  right even when the fence serialises" rule the commit cites); #870.
- **Suggested Fix**: Add one helper to
  `crates/renderer/src/vulkan/descriptors.rs` — it already owns the
  `image_barrier_*` family — e.g.
  `clear_general_accumulator(device, cmd, image, range, extra_src_stages, dst_stages)`
  that emits the whole sandwich with `TRANSFER` structurally present on both
  sides, and route all four sites through it. Then collapse
  `skip_clear_mask_pin_tests` into a single assertion over the helper rather
  than three `include_str!` scans that must each be remembered.
- **Effort**: small (≤2 h)

---

---

### TD2-2026-09-05-02: `parse_skyrim_shader_base` is the shared Skyrim+ shader head, but its two inline twins got #2603's gap-band predicates and it did not


- **Severity**: MEDIUM (promotion trigger: divergent bug-fix history)
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - `crates/nif/src/blocks/shader.rs` — `parse_skyrim_shader_base` (the helper; consumed by `BSSkyShaderProperty::parse` and `BSWaterShaderProperty::parse`)
  - `crates/nif/src/blocks/shader.rs` — `BSLightingShaderProperty::parse_fo4` (inline copy)
  - `crates/nif/src/blocks/shader.rs` — `BSEffectShaderProperty::parse` (inline copy)
- **Status**: NEW
- **Description**: All three read the identical six-field Skyrim+ shader head —
  `shader_flags_1`/`shader_flags_2` (typed `u32` pair), then the
  `sf1_crcs`/`sf2_crcs` CRC arrays, then `uv_offset` and `uv_scale`. A helper
  for exactly this sequence already exists in the same file
  (`parse_skyrim_shader_base` → `type SkyrimShaderBase`), but the two largest
  consumers hand-roll it. Because they are separate copies, `70f1bb74`'s #2603
  work — replacing the raw BSVER literal comparisons with the named
  `bsver::carries_typed_shader_flags` / `carries_crc_shader_flags` predicates
  that encode the BSVER-131 "neither encoding present" gap band — landed on the
  two inline copies and left the helper on the old literal gate.
- **Evidence**:
  - Helper, unchanged since before #2603:
    `if bsver < crate::version::bsver::FO4_CRC_FLAGS { (read_u32, read_u32) } else { (0, 0) }`
    — `FO4_CRC_FLAGS == 132`, so this reads the 8-byte pair at `bsver == 131`.
  - Both inline copies:
    `if crate::version::bsver::carries_typed_shader_flags(bsver) { … }` —
    `carries_typed_shader_flags(b) == (b <= FALLOUT4)`, i.e. `b <= 130`, so
    they read *nothing* at 131.
  - `crates/nif/src/version.rs` pins the partition:
    `assert!(!carries_typed_shader_flags(FO4_SHADER_GAP));`
    `assert!(!carries_crc_shader_flags(FO4_SHADER_GAP));` — 131 carries neither.
    `is_shader_flag_gap` exists solely to name that band.
  - `git show 70f1bb74 -- crates/nif/src/blocks/shader.rs` shows the two inline
    gates being rewritten from `bsver <= FALLOUT4` / `bsver >= FO4_CRC_FLAGS`
    to the predicates; `parse_skyrim_shader_base` is absent from the diff.
- **Impact**: Latent, not live: at `bsver == 131` a `BSSkyShaderProperty` or
  `BSWaterShaderProperty` would over-consume 8 bytes and drift the stream for
  the rest of the block. BSVER 131 is a dev-stream band that ships no game
  content (the version.rs doc comment says so), so nothing in the corpus hits
  it today. The real cost is that the codebase now holds three *different*
  answers to "does this BSVER carry typed shader flags", and the test that pins
  the partition (`bsver_shader_flag_band_tests`) validates the predicates, not
  the one parse site that bypasses them.
- **Related**: #2603 (CLOSED, `70f1bb74`), #409 (the original gap-band
  discovery), #713 (which created `parse_skyrim_shader_base`).
- **Suggested Fix**: In `crates/nif/src/blocks/shader.rs`, change
  `parse_skyrim_shader_base`'s two gates to
  `bsver::carries_typed_shader_flags(bsver)` /
  `bsver::carries_crc_shader_flags(bsver)`, then route
  `BSLightingShaderProperty::parse_fo4` and `BSEffectShaderProperty::parse`
  through the helper for the six shared fields (both continue their own tails
  unchanged after it). That leaves exactly one copy of the gate, already covered
  by `bsver_shader_flag_band_tests`.
- **Effort**: small (≤2 h)

---

---

### TD3-2026-09-05-01: `bindings.glsl` documents `GpuMaterial` as 396 B and points the struct-sync invariant at `gpu_material_size_is_396_bytes` — a test that has never existed (live: 432 B / `_432_`)


- **Severity**: MEDIUM
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `crates/renderer/shaders/include/bindings.glsl:99` and `:107-108`
- **Status**: **Regression of #1755** (CLOSED — *"TD3-002: bindings.glsl cites stale `gpu_material_size_is_260_bytes` test (real: `_300_bytes`)"*). Same file, same comment block, same defect, different numbers.
- **Effort**: trivial (≤30 min)
- **Age**: `d9d4a6d7` introduced the "396 B" wording; `ceb69d24` (2026-08-25) grew the struct 396 → 432 B and updated the Rust side but not this GLSL mirror. **11 days stale.**
- **Description**: `bindings.glsl` is the *single* GLSL declaration of `struct GpuMaterial` (lifted out of `triangle.frag` under #1583/#1590) and is therefore the one file whose header comment carries the Rust↔GLSL lockstep contract. Two sentences in that header are wrong:
  1. Line 99 — `// Mirrors the Rust `GpuMaterial` (396 B std430) defined` — the struct is **432 B**.
  2. Lines 107-108 — `// `intern`/encoding sites; the size of this struct (396 B) is pinned by` / `// `gpu_material_size_is_396_bytes` on the Rust side.` — the size is wrong **and** the named test does not exist.
- **Evidence**:
  ```
  $ grep -rn "gpu_material_size_is_396" --include='*.rs' --include='*.glsl' .
  crates/renderer/shaders/include/bindings.glsl:108:// `gpu_material_size_is_396_bytes` on the Rust side.
  ```
  One hit, and it is the comment itself — there is no such test anywhere in the workspace. The live pin is `crates/renderer/src/vulkan/material_tests.rs:62-63`:
  ```rust
  fn gpu_material_size_is_432_bytes() {
      assert_eq!(std::mem::size_of::<GpuMaterial>(), 432);
  ```
  The Rust side already documents this correctly — `crates/renderer/src/vulkan/material.rs:40-46` reads *"std430 GPU-side material record. **432 bytes** per material. … → 396 B (BGEM v21+ glass optics) → 432 B (Bethesda lighting response + canonical mask roles). Pinned by `gpu_material_size_is_432_bytes`."* Only the GLSL mirror lagged.
- **Impact**: This is the highest-value doc site in the renderer for this class. `feedback_shader_struct_sync.md` names `bindings.glsl` as the #1 source of silent GPU-struct desync, and the comment's whole job is to tell a contributor which test to update in lockstep with a field addition. It currently sends them to a dead grep — exactly the failure #1755 was closed to prevent, now recurred one size-bump later. `#[repr(C)]` GPU-struct drift is HIGH per `_audit-severity.md`; a doc that misdirects the guard against it is MEDIUM per the tech-debt promotion table ("stale `GpuMaterial` size in a doc comment — lockstep-drift bait").
- **Related**: #1755 (the identical prior regression, 260→300), #1321 (`GpuMaterial` 260 B in 8 sites), #3830/#3831/#3832 (today's other shader/doc rot — distinct subjects).
- **Suggested Fix**: Two edits: `396 B std430` → `432 B std430` (line 99) and `(396 B) is pinned by` / `gpu_material_size_is_396_bytes` → `(432 B) is pinned by` / `gpu_material_size_is_432_bytes` (lines 107-108). Given this is the second recurrence of the same sentence in the same file, consider making the size a generated value: `crates/renderer/build.rs` already emits `shaders/include/shader_constants.glsl` from `shader_constants_data.rs`, so `GPU_MATERIAL_SIZE_BYTES` could be emitted alongside and the comment could stop hand-carrying a number.

---

---

### TD4-2026-09-05-01: `_audit-common.md`'s `crates/sdk` layout row understates the crate ~50× — 282 LOC / 2 files against a live 14,050 LOC / 25 files, in an un-owned crate


- **Severity**: MEDIUM
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/_audit-common.md:93` (also `:81`, the `studio_host.rs` sibling row)
- **Status**: NEW (follow-on to #3457, CLOSED — see Related)
- **Effort**: trivial (≤30 min)
- **Age**: row introduced `21a840d5`-era, filed as #3457 and landed 2026-08-27; drift accrued over the 9 days since.

**Description**
The Project Layout row reads:

```
SDK / Studio:    crates/sdk/src/ (lib.rs + studio.rs, 282 LOC, `21a840d5` 2026-08-25) —
                 renderer-independent tooling surface; `StudioSession` is a Resource.
```

The crate is no longer two files. It is **25 files / 14,050 LOC**:

```
$ ls crates/sdk/src/ | wc -l
25
$ find crates/sdk/src -name '*.rs' -exec wc -l {} + | tail -1
 14050 total
```

with `compatibility.rs` alone at ~3,760 production LOC — a file this very skill's
Phase-1 snapshot names as one of the twelve >2000-production-LOC offenders.
`git log --since=2026-08-25 -- crates/sdk/` shows ~20 feature commits (actor values,
equipment, factions, form lookup, UI/menu state, input mappings, StorageUtil
aliasing) landed after the row was written.

The sibling `studio_host.rs` row (`:81`) has the same defect at smaller scale:
252 LOC claimed, **402** live.

**Evidence — this already misled an audit inside the 90-day window.**
`_audit-common.md` is the shared layout map every audit skill is told to read
instead of re-deriving structure, and `crates/sdk` is on that same file's
**un-owned-subsystems** list (no owner audit skill). The combination means the
only structural signal any auditor gets for this crate is the 282-LOC row.
`/audit-scripting` hit exactly that and had to carry an out-of-band correction:

```
.claude/commands/audit-scripting/SKILL.md:44-49
  … plus a new `crates/sdk` crate (~14k LOC) exposing canonical engine state
  (actor values, equipment, plugin/load-order metadata, faction relationships,
  form lookups, UI/menu state, input mappings) to provider calls. None of this
  is in the **Crates** or **Engine-side wiring** lists below, no dimension's
  entry-points/checklist mentions it …
```

That is a downstream skill compensating in prose for a wrong number in the
shared map — which satisfies the severity table's *"stale baseline that misled
an audit in the last 90 days → MEDIUM"* promotion trigger.

**Impact**
Understates required coverage by ~13.8k LOC on a crate with no owner audit,
whose only listed reviewers are "per-domain owner + `/audit-ecs`". An auditor
budgeting effort from this row will treat `crates/sdk` as a rounding error.
Blast radius is every audit that consults the layout map — i.e. all 28.

**Related**
#3457 (CLOSED — *"`_audit-common.md`'s Project Layout … gives `crates/sdk` no
layout row"*; the row it added is now the misleading artifact, so this is a
follow-on, not a re-file). #3497 (CLOSED — `crates/sdk` unscanned by the save
completeness guard, same blind-spot family). #3744 (CLOSED — consolidated skill
drift; explicitly semantic-claims only, no LOC figures, so no overlap).

**Suggested Fix**
Rewrite the row from the live tree: file count, LOC, and the actual module
groups (`actor_values`, `inventory`, `factions`, `perks`, `compatibility`,
`projection`, `storage`, …) rather than `lib.rs + studio.rs`. Then delete
`audit-scripting/SKILL.md:44-49`'s compensating paragraph, since its whole
purpose is to route around this row. Consider dropping the LOC literal
altogether in favour of "re-run `find crates/sdk/src -name '*.rs' | wc -l`" —
the same treatment #2420 applied to the crate count after it went stale twice.

---

---

### TD6-2026-09-05-01: `skyrim_ruleset` / `oblivion_ruleset` are production-unreachable — `build_ruleset` silently returns `None`, and #3170's landed fix never reached `main`


- **Severity**: MEDIUM (promoted from the LOW default — reachable from a shipped CLI flag *and* a smoke test; reachability traced below)
- **Dimension**: 6 — Stub & Placeholder Implementations
- **Status**: **Regression of #3170** (closed 2026-08-25; fix present only on an unmerged branch)
- **Effort**: small (≤2 h) — the fix is already written; the work is merge + re-verify
- **Age**: `bbd501a1` (2026-08-25) is the commit that *would* have fixed it; the gap itself predates CHARAL's profile split

**Location** (symbol-anchored)

| Symbol | File |
|---|---|
| `enum RulesetBuilder` (`:47`) | `crates/core/src/character/profile.rs` |
| `CharacterRulesProfile::OBLIVION` (`:82`) / `::SKYRIM` (`:119`) | `crates/core/src/character/profile.rs` |
| `CharacterRulesProfile::build_ruleset` (`:178`) | `crates/core/src/character/profile.rs` |
| `skyrim_ruleset` (`:116`) | `crates/core/src/character/skyrim.rs` |
| `oblivion_ruleset` (`:101`), `oblivion_pool_regen_config` (`:159`) | `crates/core/src/character/tes.rs` |
| `build_character_ruleset` (`:228`) | `byroredux/src/npc_spawn.rs` |

**Description**

`RulesetBuilder` has four variants — `None`, `Fallout3`, `FalloutNewVegas`,
`Fallout4`. There is **no `Skyrim` and no `Oblivion` arm**. Both
`CharacterRulesProfile::SKYRIM` and `::OBLIVION` therefore carry
`ruleset: RulesetBuilder::None`, and `build_ruleset` hits:

```rust
RulesetBuilder::None => return None,
```

That is the silent-stub shape this dimension hunts: no panic, no `log::warn!`, no
`TODO` — just a `None` that propagates all the way to "the resource was never
inserted". Meanwhile `skyrim_ruleset()` and `oblivion_ruleset()` are **complete,
tested builders** sitting one module away with **zero production call sites**
(only their own unit tests and `crates/plugin/src/esm/records/tests.rs`).

`skyrim_ruleset` is not merely written — it is verified against real game data.
`crates/plugin/src/esm/records/tests.rs` builds it from a live `Skyrim.esm` AVIF
index and asserts `rs.derived_row_len() == 2` with the message *"one or more
Skyrim.esm AVIF EditorIDs failed to resolve"*. The builder works; nothing calls it.

**The closed-issue twist.** #3170 (`CHAR-2026-08-20-D3-01`, MEDIUM) named this exact
`RulesetBuilder`-has-no-`Skyrim`-arm gap and was closed 2026-08-25 by commit
`bbd501a1`, whose subject reads *"Fix #3170: **wire a Skyrim RulesetBuilder arm** so
#2942's GMST-sourcing seam reaches production"*. That commit is **not on `main`**:

```
$ git merge-base --is-ancestor bbd501a1 main && echo YES || echo NO
NO
$ git branch -a --contains bbd501a1
  fix/npc-spawn-dead-code-oblivion-ignore-charal-gmst
  remotes/origin/fix/npc-spawn-dead-code-oblivion-ignore-charal-gmst
```

`main` at HEAD still has the four-variant enum. The sibling fix in the same commit
(#3169, `SkillSet::SKYRIM` Illusion → `AVMysticism`) **did** reach `main` via another
route (`crates/core/src/character/skill.rs:123,141-149`), so this is a partially-applied
branch, not a wholly-forgotten one — which is exactly why it reads as done.

**Evidence — reachability trace (not asserted, walked)**

1. Shipped CLI flags `--game skyrim` / `--esm Skyrim.esm` (both in `README.md#run`)
   → `parse_esm` → `crates/plugin/src/esm/records/mod.rs:156`:
   `GameKind::Skyrim => CharacterRulesProfile::SKYRIM`.
2. `byroredux/src/cell_loader/references/mod.rs:308-313` — the once-per-load CHARAL
   construction site — calls `crate::npc_spawn::build_character_ruleset(record_index)`
   and inserts only `if let Some(rs)`.
3. `build_character_ruleset` → `index.character_rules.build_ruleset(resolve, gmst)`
   → `profile.rs:187` `RulesetBuilder::None => return None`.
4. No `CharacterRuleset` resource is ever inserted on Skyrim or Oblivion.

Smoke-test reachability: `docs/smoke-tests/p2-melee-core.sh` defaults to
`skyrim_se` (`# ... default \`skyrim_se\``, line 11) and is the gate ROADMAP cites for
"P2 combat core landed 2026-08-16".

Downstream consumers all degrade silently through the same `let … else` shape:

- `byroredux/src/combat.rs::melee_damage_charal_bonus` (`:453`) → `return 0.0`
- `crates/core/src/character/regen.rs::pool_regen_tick_system` (`:152`, a registered
  `Stage::Update` exclusive at `byroredux/src/boot.rs:1130-1140`) → `return`
- `crates/scripting/src/condition.rs:500,671` — the CTDA `GetActorValue` derived-stat
  fallback

**Second symbol in the same cluster** (reported here rather than double-filed under
Dim 8, per cross-dimension dedup): `oblivion_pool_regen_config`
(`crates/core/src/character/tes.rs:159`) has **zero call sites anywhere in the
workspace — not even a test**. The only three references are its definition, the
`pub use` re-export in `crates/core/src/character/mod.rs:121`, and a doc link at
`regen.rs:122`. Both `insert_resource(PoolRegenConfig { … })` sites in `regen.rs`
(`:255`, `:396`) are inside `#[cfg(test)] mod tests` (opens at `:224`), so
`PoolRegenConfig` is never inserted in production on **any** game and
`pool_regen_tick_system` is a permanent no-op engine-wide.

**Impact**

On Skyrim SE — the game the P2 vertical slice, `p2-melee-core.sh`,
`p1-character-traversal.sh` and `p0-door-interaction.sh` all target:

- No Magicka/Stamina regen (`pool_regen_tick_system` inert).
- No derived-stat rows for CTDA `GetActorValue`, so condition-gated content that
  asks for a derived value silently reads nothing.
- `LevelingModel::with_gmst` never runs — Skyrim's `SkillXp` variant is the *only*
  arm `with_gmst` handles, so #2942's whole GMST-sourcing seam has zero production
  reach on every shipped game (this is #3170's original subject, still true at HEAD).

Blast radius is bounded — nothing crashes, and `docs/feature-matrix.md:250,258-260`
records the state accurately ("~ built, unwired"; "`RulesetBuilder` enum has no
Oblivion/Skyrim arm"). The debt is that a **written, reviewed, issue-closing fix is
sitting unmerged** while the issue reads CLOSED, so no tracking surface will ever
surface it again.

**Related**

- #3170 (CLOSED 2026-08-25) — the issue this regresses; its fix is the unmerged commit.
- #3768 (CLOSED) — documented the *Oblivion* half as doc rot only. Its own
  Completeness Check names the unchecked sibling: *"the Skyrim paragraph in the same
  §5, which has the same `RulesetBuilder::None` shape"*. That sibling check is what
  this finding closes.
- #2941 (CLOSED) — same defect class, previously fixed for FO3
  (*"fallout3_ruleset and LevelingModel::FO3 are unreachable"*).
- User memory `orphan_branch_unmerged_fixes.md` records #2266/#3084/#3170/#3169 as
  closed-with-unmerged-fixes; this finding is the code-level confirmation for #3170,
  and shows #3169 is *not* affected. **Cross-dimension note for the merge phase**:
  #2266 (dead NPC-spawn wrappers) and #3084 (Oblivion corpus ignore-gate) belong to
  Dim 8 and Dim 9 respectively and should be re-verified there — #2266's wrappers
  appear absent from `main`, so the branch is partially applied and each issue needs
  checking on its own, not as a block.

**Suggested Fix**

Cherry-pick `bbd501a1`'s `RulesetBuilder::Skyrim` arm onto `main` (add the variant,
map `CharacterRulesProfile::SKYRIM.ruleset` to it, dispatch to `skyrim_ruleset`),
then re-run `p2-melee-core.sh`. Oblivion stays `None` on purpose — per #3768 it is
additionally blocked on a pre-`AVIF` legacy actor-value resolver — but that arm's
absence should carry a one-line comment saying so, so the next reader does not read
`RulesetBuilder`'s shape as an oversight in both directions. Separately, either wire
`oblivion_pool_regen_config` or delete it; a `pub fn` with zero call sites including
tests is unverified code.

---

---

### TD9-2026-09-05-01: The only pixel-level render regression guard cannot pass — its baseline predates FSR3 becoming the default upscaler *and* predates the bench mode it now invokes

- **Severity**: MEDIUM
- **Dimension**: Test Hygiene (Dim 9) — `test-gap`
- **Location**: `/mnt/data/src/gamebyro-redux/byroredux/tests/golden_frames.rs` (`cube_demo_golden_frame`, `run_engine_screenshot`, `MAX_DIFF_PCT`, `MAX_CHANNEL_DELTA`); baseline `/mnt/data/src/gamebyro-redux/byroredux/tests/golden/cube_demo_60f.png`
- **Status**: **Regression of #1320** (whose TH6-NEW-03 arm was precisely "regenerate `cube_demo_60f.png` against HEAD shader state")
- **Description**: `cube_demo_60f.png` was last regenerated by `4376f7a6` on **2026-06-04** (#1320). Since that commit, `git log --since=2026-06-04 -- crates/renderer/` counts **550 renderer commits**, of which **272 touch `crates/renderer/shaders/`**. Two of those changes are not "drift" but hard invalidations of the baseline:

  1. **The capture invocation changed after the capture.** `run_engine_screenshot` today passes `--bench-mode renderer-static`, a flag introduced by `f19f7f15` on **2026-08-11** — ten weeks *after* the baseline was taken. Before that commit the harness ran with "the determinism env var set" and did **not** fix delta-time at zero. The stored PNG is therefore a frame from a different timing regime than the one the test now renders.
  2. **The render path is no longer the one that produced the baseline.** `parse_args` (`byroredux/src/cli_args.rs`) defaults `--upscaler` to `"fsr3"`, and `UpscalerMode::default()` is `Fsr3(FsrQuality::Quality)`. `cube_demo_golden_frame` passes no `--upscaler`, so it renders at a reduced internal resolution and reconstructs through the vendored FidelityFX upscaler — a post-chain that did not exist when the baseline was captured (FSR3 landed 2026-07-22 onward). The compare thresholds are `PIXEL_TOLERANCE = 8`, `MAX_DIFF_PCT = 1.0`, `MAX_CHANNEL_DELTA = 32`; a native-TAA-vs-FSR3-Quality reconstruction blows past all three.

  Because the test is `#[ignore = "requires Vulkan device + release build; opt-in via --ignored"]`, nothing surfaces this: the default lane never runs it and the `--ignored` lane is hand-driven.
- **Evidence**:
  - `git log --format='%h %ad' --date=short -- byroredux/tests/golden/cube_demo_60f.png` → newest entry `4376f7a6 2026-06-04`.
  - `git show f19f7f15 -- byroredux/tests/golden_frames.rs` adds `"--bench-mode", "renderer-static"` to the `Command::args` list (2026-08-11).
  - `crates/renderer/src/vulkan/upscaling.rs`: `fn default() -> Self { Self::Fsr3(FsrQuality::Quality) }`; `byroredux/src/cli_args.rs`: `option("--upscaler")?.unwrap_or("fsr3")`.
  - Every CLI flag the harness passes still parses, so the failure mode is a *pixel mismatch*, not a crash — i.e. it will read as "the renderer regressed", not "the test is stale".
- **Impact**: ByroRedux has exactly one end-to-end pixel regression guard and it is unusable. The next person to run `cargo test --release -p byroredux -- --ignored golden` gets a red that is 100 % baseline staleness, and the documented recovery (`BYROREDUX_REGEN_GOLDEN=1`) *rewrites the baseline* — so the natural response destroys whatever signal remained. Blast radius is the whole renderer: 550 commits of shader/pipeline work (ReSTIR, volumetrics tuning, bloom, TAA, the FSR3 presentation chain, `GpuMaterial` 300 → 432 B) have shipped with no image-level gate at all.
- **Related**: #1320 (original stale-golden finding, CLOSED); #3003 (the sibling "smoke gate exits 0 when data is absent" defect, whose fix is the precedent cited in TD9-2026-09-05-02); memory note *Speculative Vulkan Fixes* — a golden frame is the cheapest non-RenderDoc evidence this project has, which is exactly why letting it rot is expensive.
- **Suggested Fix**: Pin the deterministic path in `run_engine_screenshot` by adding `"--upscaler", "taa"` to the arg list — an FFI upscaler whose output can shift with driver version is structurally unsuitable as a golden reference — then regenerate the baseline once against current HEAD and commit it. Add a comment recording the exact invocation the stored PNG was captured with, so the next flag addition is visibly a baseline-invalidating change.
- **Effort**: **Small** (one arg + one `BYROREDUX_REGEN_GOLDEN=1` run on the dev GPU; ~30 min including eyeballing the regenerated frame).

---

---

### TD9-2026-09-05-02: At least 101 of 182 `#[ignore]`d real-data tests report a green `ok` when their data is absent — the Rust half of the tree has no skip signal, while the shell half already does

- **Severity**: MEDIUM
- **Dimension**: Test Hygiene (Dim 9) — `test-gap`
- **Location**: tree-wide; densest clusters at `/mnt/data/src/gamebyro-redux/crates/plugin/tests/parse_real_esm.rs` (19 tests, helper `data_dir`), `/mnt/data/src/gamebyro-redux/crates/plugin/src/esm/cell/tests/integration.rs` (12), `/mnt/data/src/gamebyro-redux/crates/bsa/src/archive/tests.rs` (10), `/mnt/data/src/gamebyro-redux/crates/bsa/tests/ba2_real.rs` (7), `/mnt/data/src/gamebyro-redux/byroredux/src/npc_spawn/tests.rs` (6), `/mnt/data/src/gamebyro-redux/crates/plugin/src/esm/records/tests.rs` (6)
- **Status**: NEW (the surviving half of #3084's premise; sibling of #3003, which fixed exactly this defect on the *shell* gates)
- **Description**: The universal idiom for a data-gated Rust test in this repo is `#[ignore = "needs <GAME> game data on disk"]` **plus** an in-body `eprintln!("… skipping …"); return;`. The `#[ignore]` handles the default lane correctly. But the `--ignored` lane — the *only* lane these tests ever execute in — has no skip result: libtest reports `test … ok`, and without `--nocapture` it swallows the `eprintln!` for passing tests. So on any machine that lacks one title's data (i.e. every machine, for at least some titles), an operator running `cargo test -p byroredux-plugin -- --ignored` reads N passes and learns nothing about which of them touched a byte of real data.

  This is precisely the defect `docs/smoke-tests/README.md` names for the shell gates and forbids there: *"an explicit `SKIP` with exit code `77`, never a pass"*, with `.github/workflows/playable-smoke.yml` turning a 77 into `::error::… skipped because $GAME data is unavailable`. The Rust corpus tests have no equivalent, and **no strict/require mode exists anywhere** — `grep -rnoE 'BYRO(REDUX)?_[A-Z0-9_]*(REQUIRE|STRICT|MUST)[A-Z0-9_]*'` over the tree returns nothing.

  A second-order defect in the same helper compounds it: `data_dir` (`crates/plugin/tests/parse_real_esm.rs`) treats an explicitly-set-but-wrong env var as advisory — it `eprintln!`s "falling back to default" and then reads the **hardcoded `/mnt/data/SteamLibrary/...` path anyway**. An operator who points `BYROREDUX_FNV_DATA` at a modded or DLC-stripped install silently gets results from a different install than the one they named.
- **Evidence**: Programmatic sweep over all 7 310 `#[test]` fns: 182 carry `#[ignore]`; **101** of those contain both a skip-ish diagnostic (`skip` / `not available` / `missing`) and a bare `return;` in the body. 101 is a **floor** — the sweep does not catch `let Ok(..) = .. else { return }` forms with no diagnostic word. Representative shape, `crates/plugin/tests/parse_real_esm.rs`:
  ```rust
  #[ignore = "needs FNV game data on disk"]
  fn fnv_karma_good_global_decodes_float_payload_before_narrowing() {
      let Some(dir) = data_dir("BYROREDUX_FNV_DATA", FNV_FALLBACK) else {
          eprintln!("[FNV/GLOB] skipping: game data unavailable");
          return;                 // ← libtest records `ok`
      };
  ```
  Contrast `docs/smoke-tests/README.md:8`: *"an explicit `SKIP` with exit code `77`, never a pass."*
- **Impact**: Not a coverage gap — a **trust gap**: a green `--ignored` run is not evidence. This repo has already been burned three times by mis-read test signals in this exact area (#3440 and #3456 were both wrong `#[ignore]` baselines published inside an audit report; #3348 was a red `--ignored` lane nobody noticed). The auto-memory note *NIF Corpus Baseline Tests* records the live consequence: "FO76 currently silently RED on NiPSysBlock" — a corpus baseline whose status had to be tracked in a memory file because the test run itself does not say. Worst case, a real-data guard is dropped or broken and the `--ignored` lane stays green for months.
- **Related**: #3084 (the `#[ignore]` half, fixed); #3003 (identical defect on the shell gates, fixed with exit 77 — the precedent); #3348 (red `--ignored` lane on `byroredux-bsa`); #3440 / #3456 / #3749 (the wrong-baseline lineage); memory note *NIF Corpus Baseline Tests*.
- **Suggested Fix**: Introduce one strict switch — e.g. `BYROREDUX_REQUIRE_GAME_DATA=1` — read by the shared resolvers (`data_dir` in `parse_real_esm.rs`, `game_data_dir` in `crates/nif/tests/common/`, and their siblings) so a missing corpus `panic!`s instead of returning. Set it in whatever lane is meant to be authoritative, exactly as `playable-smoke.yml` promotes exit 77 to an error. Separately, make `data_dir` treat an explicitly-set env var as binding: if it names a non-directory, fail rather than silently substituting the hardcoded Steam path.
- **Effort**: **Medium** (one helper each in ~4 resolver sites, then a mechanical sweep of the ~101 call sites to route through them; no test logic changes).

---

---

## LOW

### TD1-2026-09-05-02: `compatibility.rs` is 3759 production LOC and 55 % StorageUtil — and the SKILL's proposed `ExtenderFamily` split axis does not exist in the code


- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/sdk/src/compatibility.rs` (3759 production / 5344 total LOC)
- **Status**: NEW
- **Age**: created `287f270f`, 2026-08-31 ("feat(scripting): preflight extender calls") — 246 total LOC at birth, **5344 today across 35 commits in 5 days** (a 21× growth)
- **Description**: The audit skill proposes splitting this file "one module per
  `ExtenderFamily::{Skse,F4se,Xnvse,Obse,PapyrusUtil,JContainers,Shared}`". **That axis is wrong.**
  `ExtenderFamily` is a metadata tag on `SourceAlias` / `CompatibilityMatch`, not an organizing
  principle: it appears on 30 of 3759 production lines, and 23 of those 30 are inside the two
  classifier functions `classify_static_call` and `classify_obscript_command`. Splitting on it would
  produce one ~160-line module and six near-empty ones while leaving the real 2000-line mass intact.
- **Evidence**:
  ```
  $ grep -nE 'ExtenderFamily' crates/sdk/src/compatibility.rs | awk -F: '$1<3760' | wc -l
  30
  ```
  The file's actual axis is **service surface**, and it repeats the same four-layer stack per service:
  1. route constants — `PAPYRUS_GAME_*_ROUTE`, `PAPYRUS_INPUT_*_ROUTE`, `PAPYRUS_UI_*_ROUTE`,
     `PAPYRUS_STORAGE_UTIL_*_ROUTE`, `PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX`, `PAPYRUS_MOD_EVENT_ROUTE_PREFIX`;
  2. declaration builders — `papyrus_game_content_declarations`, `papyrus_input_declarations`,
     `papyrus_ui_declarations`, `papyrus_storage_util_declarations`,
     `papyrus_storage_util_list_declarations`, `papyrus_storage_util_prefix_declarations`,
     `papyrus_legacy_container_declarations`, `papyrus_mod_event_declarations`;
  3. source-alias classifiers — `obscript_source_alias`, `method_source_alias`, `source_alias`,
     `storage_util_prefix_source_alias`, `storage_util_list_source_alias`,
     `legacy_container_source_alias`, `classify_obscript_command`, `classify_static_call`;
  4. runtime adapters — `adapt_papyrus_game_*` (11), `adapt_papyrus_input_*`, `adapt_papyrus_ui_*`,
     `adapt_legacy_obscript_load_order`, `adapt_legacy_send_mod_event`,
     `adapt_storage_util_global_{scalar,prefix,list,form_filter}` + the `checked_*` /
     `encode_*` / `decode_*` / `parse_storage_util_*_route` codec helpers.

  **PapyrusUtil StorageUtil alone touches 505 of the 3759 production lines by name** and owns the
  file's whole type vocabulary (`StorageUtilScalarCall/Result/Adaptation/AdapterError`,
  `StorageUtilList{Kind,Value,Call,Result,Adaptation,Operation}`,
  `StorageUtilPrefix{Kind,Operation,Adaptation}`) plus all three of the file's >200-LOC functions:
  `adapt_storage_util_global_list` (384), `papyrus_storage_util_declarations` (251),
  `papyrus_storage_util_list_declarations` (245). Legacy containers (37 lines by name) and mod
  events (43) are comparatively tiny.
- **Impact**: as with `extensions.rs`, this is the fastest-growing debt in the workspace, not
  settled debt. Secondary impact: the wrong axis is currently written into
  `.claude/commands/audit-tech-debt/SKILL.md`, so the next auditor who trusts it will propose a
  refactor that does not reduce the file (report that half under **Dimension 4**).
- **Related**: TD1-…-01 (`extensions.rs` holds the *invocation* side of the same StorageUtil surface);
  TD1-…-03 (`papyrus_provider.rs` holds the *lowering* side); TD1-…-10 (the 106-arm match in this file).
- **Suggested Fix**: `compatibility/{mod,routes,declarations,source_alias,game_content,input_ui,storage_util,legacy_containers,mod_events}.rs`,
  taking `storage_util.rs` first — it is the only extraction that meaningfully shrinks the file
  (~2000 LOC), and it is self-contained because its types are used nowhere else in the crate.
  Keep every `pub` symbol re-exported from `compatibility::` so `byroredux/src/extensions.rs`'s
  50-symbol `use byroredux_sdk::compatibility::{…}` block does not have to change.
- **Effort**: medium

---

---

### TD1-2026-09-05-03: `papyrus_provider.rs` is a compiler front-end, an IR, and an interpreter in one 3711-LOC file


- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/scripting/src/papyrus_provider.rs` (3711 production / 6158 total LOC)
- **Status**: NEW
- **Age**: created `6254d996`, 2026-09-01 ("feat(scripting): lower typed Papyrus provider calls") — 350 total LOC at birth, **6158 today across 51 commits in 4 days**
- **Description**: The module doc says it "resolves a legal `Provider.Function(...)` … to the
  principal-qualified SDK route and validates typed arguments, but it never enters Wasm or touches
  the ECS while lowering." That describes the *first half* of the file. The second half is a
  full statement interpreter that does touch the ECS: `papyrus_provider_system` (361 LOC — the
  file's only >200-LOC function), `execute_statements`, `evaluate_condition`, `evaluate_provider_value`,
  `apply_provider_arithmetic`, `compare_condition_values`, `materialize_provider_arguments`. The
  lowering half and the execution half are roughly equal in size and share only the IR types.
- **Evidence**: the file falls into five contiguous, non-interleaved regions:

  | Region | Symbols (first → last) | ≈LOC |
  |---|---|---|
  | runtime plumbing | `PapyrusProviderRuntime` → `register` | 130 |
  | catalog | `PapyrusProviderRoute` → `PapyrusProviderCatalog::contains_provider` | 120 |
  | call lowering (front-end) | `TypedPapyrusProviderCall` → `lower_literal` (incl. `storage_util_arity`, `legacy_container_arity`, `validate_*_arity`, `lower_provider_invocation` at 194 LOC) | 650 |
  | IR + resources | `PapyrusProviderEvent` → `PapyrusProviderHandler::projected_mod_event_locals` | 400 |
  | program lowering (AST → IR) | `lower_provider_program` → `resolve_mod_event_senders` (incl. `lower_statements` at 152 LOC, `lower_condition_at_depth`, `sdk_type`, `default_value`) | 1130 |
  | execution (back-end) | `papyrus_provider_system` → `compare_ordered` | 1160 |

  `MAX_PROVIDER_HANDLER_NESTING`/`MAX_PROVIDER_CONTINUATIONS`/`MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS`/
  `MAX_PENDING_PAPYRUS_MOD_EVENTS` are the only symbols the two halves genuinely share besides the IR.
- **Impact**: the two halves have different test shapes (lowering is pure and table-testable;
  execution needs a `World`) but currently share one 6158-line `#[cfg(test)]` boundary, so every
  lowering test recompiles the interpreter. Same 4-day growth-rate amplification as TD1-…-01/02.
- **Related**: TD1-…-02 (the `byroredux_sdk::compatibility` route constants it imports 24 of).
- **Suggested Fix**: `papyrus_provider/{mod,runtime,catalog,ir,lower_call,lower_program,execute}.rs`.
  The IR module is the natural seam — it is what both halves import and nothing else does. Nothing
  crosses a lock or scheduler boundary, so the move is mechanical.
- **Effort**: medium

---

---

### TD1-2026-09-05-04: `mod-runtime/runtime.rs` holds 19 separate `impl <wit>::Host for HostState` blocks in one 3495-LOC file (the SKILL's per-binding axis is CORRECT — verified)


- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/mod-runtime/src/runtime.rs` (3495 production / 4588 total LOC)
- **Status**: NEW
- **Age**: created `9f619355`, 2026-08-06 — 312 total LOC at birth, **4588 today across 37 commits**
- **Description**: Unlike TD1-…-02, the axis the skill guessed here holds up under inspection. The
  file's WIT host surface is already physically partitioned into one `impl` block per interface;
  they are simply all in the same file. Rust allows `impl Trait for Type` in any module of the
  owning crate, so this is a pure relocation with no signature changes.
- **Evidence**: the 19 host-impl blocks, in file order —
  `events`, `state`, `wit_legacy_containers`, `wit_storage`, `world_state`, `content_catalog`,
  `actor_values`, `inventory`, `factions`, `faction_relationships`, `perks`, `packages`,
  `animation`, `reputation`, `world_spatial`, `script_functions`, `console`, `logging`, `context`
  — spanning ≈`:1591`–`:3495`, i.e. **~1900 of the 3495 production lines**. The remainder is:
  - `SandboxRuntime` (`new` / `config` / `catalog` / `compile` / `instantiate`) ≈570 LOC;
  - `ModInstance` (`initialize`, the ten `on_*` guest entry points, the `set_*_snapshot` setters,
    `reject_deferred_commands`, `shutdown`, `enter`, `quarantine`) ≈750 LOC;
  - `struct HostState` + its 17 `require_*` capability guards (`require_actor_values_read` …
    `require_storage_write`) ≈250 LOC — **the crate's trust boundary, currently buried between
    two host-impl blocks**;
  - ~25 free SDK↔WIT converters (`sdk_entity_ref`, `sdk_form_ref`, `sdk_storage_key`,
    `sdk_storage_value`, `wit_actor_value_state`, `wit_inventory_snapshot`, `wit_perk_snapshot`,
    `wit_entity_projection`, …) ≈240 LOC.

  One production function exceeds 200 LOC: **`SandboxRuntime::new` (389 LOC, `:218`–`:606`)**. It is
  not a construction chain — it is a declarative registration wall (25 `register_capability` /
  `register_service` / `CapabilityDescriptor` / `ServiceDescriptor` sites) around ~15 lines of real
  wasmtime setup (`Config`, `Engine::new`, `Linker::new`, `Extension::add_to_linker`).
- **Impact**: `crates/mod-runtime` is named in `_audit-common.md` as a **trust boundary with no
  owner audit skill**. The 17 `require_*` guards are the enforcement surface for that boundary and
  they are currently unreadable as a set, because they sit at `:3148` between the `console` and
  `logging` host impls. Concentrating them in one `capabilities.rs` is a review-quality win
  independent of the LOC count.
- **Related**: `/audit-safety` Dimension 11 owns this crate incidentally; TD1-…-01 (`extensions.rs`
  is its only host-side consumer).
- **Suggested Fix**: `runtime/{mod,sandbox,instance,host_state,capabilities,convert,host/*.rs}` —
  one file per WIT interface under `host/`, the guards in `capabilities.rs`, the converters in
  `convert.rs`. Separately, lift `SandboxRuntime::new`'s descriptor list into a
  `const CAPABILITY_DESCRIPTORS: &[(&str, &str)]` + `const SERVICE_DESCRIPTORS: &[…]` table and
  loop over it — that alone removes ~350 of its 389 lines.
- **Effort**: medium

---

---

### TD1-2026-09-05-05: `fragment.rs` is 2538 production LOC of 2540 total, with a 519-LOC `apply_effect` and 18 near-identical `populate_*` entry points


- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/scripting/src/fragment.rs` (2538 production / 2540 total LOC); `apply_effect` at `:797`–`:1315`
- **Status**: NEW
- **Age**: created `9b375200`, 2026-06-23 — 244 total LOC at birth, **2540 today across 49 commits**
- **Description**: Uniquely in this bucket the file is ~100 % production — its tests already live in
  the sibling `crates/scripting/src/fragment/tests.rs` (3015 lines, 0 production). So the split
  target directory exists and is populated with exactly one file; the production side simply never
  followed. Three responsibilities are interleaved.
- **Evidence**:
  1. **Resources / state** — `ReferenceEnableState`, `QuestStageFragments`, `SceneFragments`,
     `SceneFragmentEffects`, `PendingFragmentExecution`, `FragmentResumeCondition`,
     `FragmentExecutionQueue`, `PendingFragmentActivations`, `DeferredFragmentEffects`,
     `DeferredProviderFragmentStep`, `DeferredCinematicPresentationEffect` (≈470 LOC).
  2. **Effect interpreter** — `resolve_quest`, `resolve_quest_logged`, `resolve_property_form_id`,
     `resolve_object`, `resolve_actor`, `actors_3d_loaded`, `update_actor_cinematic_state`,
     `apply_fragment_guard_free`, `poll_fragment_generated_advances`, `apply_effect`,
     `apply_quest_scoped_effect`, `apply_effects` (≈1150 LOC).
  3. **Population from `.pex` / `.psc`** — eighteen `populate_*` functions in six four-variant
     families (`populate_quest_fragments_from_pex[_detailed][_with_providers][_internal]`,
     the `populate_owned_*` twins, and the `..._from_script` and `..._scene_fragments_*` mirrors),
     plus `FragmentPexTranslation`, `OwnedFragmentProviders`, `FragmentProviderScope`,
     `quest_property_names`, `function_body` (≈580 LOC).
  4. **Dispatch systems** — `fragment_activation_flush_system`, `fragment_continuation_system`,
     `scene_fragment_dispatch_system`, `quest_fragment_dispatch_system`, `register`, `MAX_CASCADE`.

  `apply_effect` (519 LOC) is a 23-arm `match effect` where individual arms run to ~70 lines
  (`EquipItem` ≈70, `SetVehicle` ≈38, `TetherToHorse` ≈45, `Disable` ≈33, `StartScene|StopScene` ≈63).
  Under the 50-arm rule it is *not* a lookup-table candidate — the arms are behaviour, not data —
  but they group cleanly by effect family: globals · inventory (`AddItem`/`EquipItem`) ·
  placement & enable (`MoveTo`/`Disable`/`Activate`/`SetOpen`) · scene (`StartScene`/`StopScene`) ·
  player control (`SetPlayerRestrained`/`SetPlayerControls`/`SetPlayerAiDriven`/`SetHudCartMode`/
  `SetSittingRotation`/`RegisterPlayerAnimationEvent`) · vehicle-cinematic (`SetVehicle`/
  `TetherToHorse`/`SetMotionType`/`ExitCart`/`PlayIdle`) · AI (`EvaluatePackage`) · deferred
  (`Wait`/`WaitForActors3DLoaded`/`Conditional`). `apply_quest_scoped_effect` (169 LOC) is the
  second >150-LOC function.
- **Impact**: the eighteen `populate_*` variants are the most edit-prone surface here (every new
  provider/ownership flavour adds four more), and they force a recompile of the interpreter they
  do not touch.
- **Related**: `/audit-scripting` owns correctness here; this finding is size only.
- **Suggested Fix**: `fragment/{mod,state,populate,effects,systems}.rs` beside the existing
  `fragment/tests.rs`. Within `effects.rs`, give `apply_effect` one private helper per family
  above so each arm becomes a one-line delegate — the same treatment #3739/#3738 applied
  in `boot.rs`/`resize.rs`.
- **Effort**: medium

---

---

### TD1-2026-09-05-06: `boot.rs` crossed 2232 production LOC — promote #3739's five `register_*_systems` functions to five files


- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/boot.rs` (2232 production / 2670 total LOC)
- **Status**: NEW (file-level; the function-level predecessor #3739 is CLOSED and correctly so)
- **Age**: `40d533a8` origin; 1018 → 2670 total across **119 commits** — the most-edited file in this bucket
- **Description**: #3739 (`d03f7a35`, 2026-09-03) split `build_scheduler` into five per-stage
  `register_*_systems` functions. That was a function-level fix and it holds — `build_scheduler`
  is now an 18-line orchestrator: eight lines of comment, `Scheduler::new()`, five `register_*` calls. It did not, and was not meant to, move the file below the
  file-level threshold; the file crossed on unrelated growth. The follow-through is mechanical
  because #3739 already drew the boundaries.
- **Evidence**: the file's production mass, by symbol —

  | Symbol | LOC | Concern |
  |---|---|---|
  | `run` (`:74`) | 319 | process entry: settings load, event loop, `App` construction |
  | `init_tracing` (`:48`) | 26 | process entry |
  | `expand_boot_request` (`:2015`) | 36 | CLI expansion |
  | `expand_game_profile_args` (`:2189`) | 161 | CLI expansion |
  | `build_world` (`:397`) | 351 | every `insert_resource` / component registration |
  | `build_scheduler` (`:753`) | 18 | orchestrator (post-#3739) |
  | `register_early_systems` (`:771`) | 76 | Stage::Early |
  | `register_update_systems` (`:847`) | **382** | Stage::Update (+ 8 nested dispatch shims) |
  | `register_post_update_systems` (`:1231`) | **210** | Stage::PostUpdate |
  | `register_physics_systems` (`:1443`) | 47 | physics |
  | `register_late_systems` (`:1490`) | **413** | Stage::Late |
  | `install_runtime_registries` (`:1908`) | 107 | registry install |

  Five production functions exceed 200 LOC. The five `register_*` bodies total ≈1130 LOC — over
  half the file.
- **Impact**: `boot.rs` is documented in `_audit-common.md` as "the authority for *which stage does
  X run in*". At 2670 lines that authority is hard to consult, and 119 commits means nearly every
  feature lands a line here — the highest merge-conflict surface in the binary.
- **Related**: #3739 (function-level, closed); #2731 (the `main.rs` split that created this file —
  **do not re-propose splitting `main.rs`**, verified at 1267 total / 1096 production today).
- **Suggested Fix**: `boot/{mod,cli,world,registries}.rs` + `boot/schedule/{mod,early,update,post_update,physics,late}.rs`,
  moving each `register_*_systems` body verbatim. `mod.rs` keeps `run`/`init_tracing` and the
  `pub(crate)` re-exports so no call site changes. This is the lowest-risk split in the bucket —
  the boundaries already exist as function boundaries and there is a
  `byroredux/src/scheduler_access_tests.rs` guard on the result.
- **Effort**: small

---

---

### TD1-2026-09-05-07: `walk/mod.rs` crossed 2165 production LOC — split the three independent satellite walkers out (note: the SKILL's stated rationale, "per the module doc's own category list", does not exist)


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

---

---

### TD1-2026-09-05-08: `asset_provider/material.rs` crossed 2044 production LOC because `merge_external_material` grew 37 % to 931 LOC since #2412 assessed it at 678 and recommended awareness only


- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/asset_provider/material.rs` (2044 production / 2044 total — 100 % production); `merge_external_material` at `:1114`–`:2044`
- **Status**: NEW (not a regression of #2412 — that issue closed with an explicit *no-action* recommendation, so nothing was fixed to regress)
- **Age**: `b0a3fa02`, 2026-06-24; 1089 → 2044 across 38 commits. `merge_external_material` was
  **678 LOC on 2026-08-12** (#2412) and is **931 today** (+253, +37 % in 24 days)
- **Description**: #2412 measured this function at 678 LOC and concluded: *"No action recommended on
  `merge_external_material` beyond awareness — it is a deliberate single NIFAL boundary and should
  not be split in a way that weakens that invariant."* That reasoning was sound and is preserved by
  the fix below, but two things changed since: the function is now **the 5th-largest production
  function in the workspace** and **46 % of its host file**, and the host file itself has crossed
  the file-level threshold — which #2412 did not evaluate.
- **Evidence**: the function's control flow is a single three-way dispatch with two enormous arms:
  ```
  :1220   if starfield_cdb_gate && path.ends_with(".mat")   → apply_cdb_pbr_fallback, early return
  :1240-:1266  ~16 `let mut set_* = false` override sentinels
  :1295   if dispatch_kind == Some(MaterialKind::Bgsm)  { … }   ← ~456 LOC
  :1751   } else if dispatch_kind == Some(MaterialKind::Bgem) { … }  ← ~237 LOC
  :1988   } else { … }                                            ← ~38 LOC
  :2039   if touched { Merged } else { PresenceOnly }
  ```
  The rest of the file is already four separable clusters:
  - **BGSM/BGEM semantics helpers** — `forward_bgsm_phase1_flags`, `forward_bgsm_rim_subsurface`,
    `forward_bgsm_env_map_scale`, `conductor_diffuse_tint`, `bgsm_metalness`,
    `bgem_uses_glass_behavior`, `bgem_uses_thin_glass_behavior`, `bgsm_blend_to_gamebryo`;
  - **Starfield CDB** — `is_materialsbeta_cdb_path`, `SF_CDB_CACHE_MAX_ENTRIES`, `sf_cdb_cache`,
    `sf_cdb_cache_insert`, `discover_starfield_cdbs`, `cdb_scan_candidates`,
    `MaterialProvider::{register_starfield_cdb, register_starfield_cdb_probe, has_starfield_cdb}`,
    `apply_cdb_pbr_fallback`, `unresolved_material_warning`;
  - **provider + caches** — `build_material_provider`, `MaterialProvider`, `MAX_BGEM_CACHE_ENTRIES`,
    `MAX_FAILED_PATHS`, `new`, `geometry_csg`, `push_archive`, `extract_from_archives`,
    `resolve_bgsm`, `resolve_bgem`, `peek_magic`, `insert_{bgsm,bgem}_for_test`;
  - **the merge boundary** — `MergeOutcome`, `record_external_texture_sources`,
    `merge_external_material`.

  Supporting nit (cross-refer **Dimension 2**): the LRU half-eviction body
  `if len >= MAX { for _ in 0..MAX/2 { if let Some(old) = …_order.pop_front() { …remove(&old) } } }`
  is written out four times (`:735`, `:770`, `:859`, `:875`) and produces the file's only
  nesting-depth-> 5 site (`:736`, seven levels inside `resolve_bgsm`). One
  `half_evict(order, set, cap)` helper removes all four.
- **Impact**: this is the single NIFAL `ImportedMaterial` sidecar boundary — every FO4/FO76/Starfield
  material finding routes through it, and per `_audit-severity.md` a wrong translation here is HIGH
  by construction with no per-draw fallback to mask it. A 931-LOC function is a poor place to keep
  that invariant reviewable.
- **Related**: #2412 (CLOSED, awareness-only, at 678 LOC); #2709 (`MergeOutcome`, the tri-state
  return this function grew to carry); #2702 (the mirror-test defect that motivated extracting
  `forward_bgsm_*` out of this same loop — the precedent for the fix below); `/audit-nifal` owns
  correctness.
- **Suggested Fix**: **preserve #2412's invariant explicitly** — `merge_external_material` stays the
  one public entry point and the one place a sidecar can touch `&mut ImportedMaterial`. Extract only
  its two arms into private siblings, `merge_bgsm_arm(&mut ImportedMaterial, &ResolvedMaterial, &mut MergeSentinels, …)`
  and `merge_bgem_arm(…)`, with the ~16 `set_*` bools promoted to a `MergeSentinels` struct. That is
  the same extraction #2702 already performed for `forward_bgsm_phase1_flags` and for the same
  stated reason (tests reach the real logic). At file level, split
  `asset_provider/material/{mod,cdb,provider,merge}.rs`.
- **Effort**: medium

---

---

### TD1-2026-09-05-09: the #2731 / #3282 file splits produced six single-function files — the extracted functions were relocated, never decomposed


- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `byroredux/src/scene/nif_loader.rs::load_nif_bytes_with_skeleton` (1105 LOC of a 1614-line file); `crates/renderer/src/vulkan/context/build_and_upload_instances.rs::build_and_upload_instances` (919 of 1058); `byroredux/src/render/static_meshes.rs::collect_static_mesh_draws` (916 of 1339); `crates/renderer/src/vulkan/context/skinned_blas_refit.rs::record_skinned_blas_refit` (837 of 1207); `byroredux/src/app_events.rs::about_to_wait` (771 of 1310); `byroredux/src/app_frame.rs::render_one_frame` (671 of 905)
- **Status**: NEW
- **Description**: The workspace has **133 production functions over 200 LOC**. Most are the shapes
  #2412 already accepted and this finding does not re-flag them: field-proportional parsers
  (`parse_block_inner` 1102, `dispatch_blocks` 503, `parse_esm_with_load_order` 429,
  `parse_cell_group_inner` 525, `parse_refr_group_inner` 457, `parse_wrld_children_inner` 363,
  `parse_qust_alias` 358, `open` 412), Vulkan constructors (`composite::new_inner` 786,
  `volumetrics::new_inner` 774), and build/example targets (`renderer/build.rs::main` 829,
  `crates/scripting/examples/mq101_conformance.rs::run` 1445).

  The six above are a **different, newer class**: each is >59 % of a file that exists *only* to hold
  it, created by a file-level split that moved the function without decomposing it. They are the
  mirror image of the #3739/#3738 pattern the SKILL warns about (function split that did not move
  the file) — here the file split did not move the function.
- **Evidence**: `context/{build_and_upload_instances,skinned_blas_refit}.rs` came out of #1857/#3282;
  `app_events.rs`/`app_frame.rs` came out of #2731. None of these functions appears in any open or
  closed issue title. `crates/renderer/src/vulkan/context/init.rs::build_pipelines_and_finish`
  (1118 LOC) looks like a seventh but is **deliberately excluded** — `init.rs`'s module doc records
  that a finer phase-3 split was evaluated and rejected under #1749 because every value it builds
  feeds the final struct literal in the same phase; that is a downstream symptom of #3736
  (`VulkanContext`'s field count), not an independent finding.
- **Impact**: LOW individually. Reported as one census entry so the next sweep can diff the count
  (133) and so nobody reads "the file was split" as "the complexity was reduced".
- **Related**: #3739, #3738 (the inverse pattern); #2412 (the accepted-shape taxonomy this reuses);
  #3736 (owns `build_pipelines_and_finish` transitively); `feedback_speculative_vulkan_fixes.md` —
  the two `context/` entries are render-recording paths, so any decomposition must be verified in
  RenderDoc, not by `cargo test`.
- **Suggested Fix**: no bulk action. When one of these files is next touched, decompose the function
  in place along its existing comment sections rather than adding to it. Highest value first:
  `load_nif_bytes_with_skeleton` (not a Vulkan path, so testable, and it is the NIF→ECS entry point).
- **Effort**: medium (per function; do not batch)

---

---

### TD1-2026-09-05-10: `storage_util_form_type_id` is a 105-arm FourCC→i32 match that should be a static table


- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/sdk/src/compatibility.rs::storage_util_form_type_id` (`:2694`–`:2804`)
- **Status**: NEW
- **Description**: The only match over the 50-arm flag anywhere in the eight files audited. 105
  `b"XXXX" => <i32>` arms mapping Creation Engine record signatures to the legacy `FormType`
  numbering, plus `_ => return None`. It is data, not behaviour — the exact case the dimension's
  "want a lookup table" rule names.
- **Evidence**: `b"TES4" => 1` … `b"FSTS" => 111`, with `b"NPC_" | b"CREA" => 43` the sole
  many-to-one arm and 96/97/106/107 deliberately absent.
- **Impact**: minimal at runtime (the compiler builds a jump table either way). The cost is
  reviewability: a wrong or missing signature is invisible in a 105-arm wall, and the mapping has no
  second home in the workspace to cross-check against — `grep -rn 'b"KYWD" =>' crates byroredux`
  returns this site only, so it is **not** duplicated logic (checked; not a Dimension 2 finding).
- **Related**: the memory note *Record Type Catalog* (98 classes, `RecordType` uses FourCC) —
  if a canonical `RecordType` mapping is ever added to `crates/plugin`, this becomes a duplication
  finding; today it is the only copy.
- **Suggested Fix**: `const FORM_TYPE_IDS: &[(&[u8; 4], i32)]` beside the function plus a linear or
  binary search, and one test asserting the table is sorted and has no duplicate signature. Keeps
  the `NPC_`/`CREA` alias explicit as two rows.
- **Effort**: trivial

---

---

### TD1-2026-09-05-11: #2256 escalation — `volumetrics.rs` is now 2937 production LOC and `new_inner` is 774, not the 556 the issue records


- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs` (2937 production / 3863 total); `VolumetricsPipeline::new_inner` at `:960`, 774 LOC
- **Status**: **Existing: #2256** — numbers refreshed, not re-filed
- **Description**: #2256 was filed 2026-08-03 against a 2075-line file with a ~556-LOC `new_inner`.
  Both figures have moved: the file is **3863 total / 2937 production** (+42 % production) and
  `new_inner` is **774 LOC** (+39 %). The issue's suggested fix (move
  `new_inner`+`create_volume`+`initialize_layouts` into a `volumetrics/init.rs` sibling, keep the
  per-frame recording path in `volumetrics.rs`) is still the right shape — `crates/renderer/src/vulkan/volumetrics/`
  already exists and holds `noise.rs`, so the target directory is live.
- **Impact**: none new; recorded so the issue is not closed against stale numbers.
- **Suggested Fix**: update #2256's Location/Evidence lines with the 2026-09-05 figures.
- **Effort**: trivial

---

## Checked and dismissed (do not re-file)

| Candidate | Why not a finding |
|---|---|
| `crates/core/src/ecs/components/mod.rs` — 40 `pub use` (> 20 flag) | 102 lines, 81 of which are `mod`/`use`, and **zero** `fn`/`struct`/`enum`/`impl`/`const` items. A pure re-export facade for the `components/` directory — one job, not two. False positive under the rule's own rationale. |
| `crates/scripting/src/lib.rs` — 22 `pub use` (> 20 flag) | 204 lines with exactly one non-re-export item, `pub fn register(world: &mut World)`. Same verdict. |
| `crates/renderer/src/vulkan/context/init.rs::build_pipelines_and_finish` (1118 LOC) | Not a regression of #1749. The 4-phase split #1749 asked for **did land**, and `init.rs`'s module doc records that a finer phase-3 split was evaluated and rejected with a stated reason (every phase-3 value feeds the terminal struct literal). Transitively owned by #3736. |
| `byroredux/src/main.rs` | 1267 total / 1096 production — under threshold. #2731's split is real and holding. Explicitly not re-proposed. |
| `crates/renderer/src/vulkan/context/draw.rs` | 3223 total / **1759 production** — out of the primary bucket after #3282 (`7463204e`). Confirmed; not re-proposed. |
| `crates/core/src/ecs/resources/mod.rs` | 1872 total / **1285 production**. The SKILL's "1822 LOC as of 2026-08-29" was a *total* figure; the production half is well under threshold. No finding. |
| `crates/renderer/src/vulkan/material.rs` (#2257) | 2852 total / 1480 production — confirms as the false positive #2977/#3081 called it. |
| Nesting depth > 5, workspace-wide | Exactly one site clears it in the eight files audited: `byroredux/src/asset_provider/material.rs::resolve_bgsm` at `:736` (7 levels). Folded into TD1-…-08 as the LRU half-eviction duplication rather than filed separately. No other file in the bucket exceeds 5. |
| Match arms > 50 | One site workspace-adjacent: `storage_util_form_type_id` (TD1-…-10). The next largest in the bucket is a 34-arm `match (qualified_name, arguments)` in `extensions.rs::invoke_storage_util` (`:1091`) — under the flag. |

## Watchlist for the next sweep (production LOC 1700–2000)

`byroredux/src/cornell.rs` 1903 · `crates/renderer/src/vulkan/buffer.rs` 1778 ·
`crates/renderer/src/vulkan/context/draw.rs` 1759 (has crossed and re-crossed three times: #1052,
#2197/#2255, #3282 — expect a fourth) · `crates/renderer/src/vulkan/svgf.rs` 1724 ·
`byroredux/src/components.rs` 1704.

---

### TD2-2026-09-05-03: twelve hand-rolled "create image → allocate → bind → view" chains, while the buffer side of the same problem has been consolidated for a year


- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location** (each is one ~80-line copy of the same chain):
  `crates/renderer/src/vulkan/taa.rs::TaaPipeline::create_history_image` ·
  `crates/renderer/src/vulkan/svgf.rs::SvgfPipeline::create_history_image` ·
  `crates/renderer/src/vulkan/bloom.rs::create_mip` ·
  `crates/renderer/src/vulkan/caustic.rs::CausticPipeline::create_slot` ·
  `crates/renderer/src/vulkan/water_caustic.rs::WaterCausticAccum::create_slot` ·
  `crates/renderer/src/vulkan/volumetrics.rs::VolumetricsPipeline::create_volume` ·
  `crates/renderer/src/vulkan/gbuffer.rs::Attachment::allocate` ·
  `crates/renderer/src/vulkan/exposure.rs::ExposureResource::new` ·
  `crates/renderer/src/vulkan/ssao.rs::SsaoPipeline::new_inner` ·
  `crates/renderer/src/vulkan/placeholder.rs::PlaceholderImage::create` ·
  `crates/renderer/src/vulkan/frame_upscaler.rs::FrameUpscaler::create_outputs` ·
  `crates/renderer/src/vulkan/composite.rs::CompositePipeline::new_inner`
- **Status**: NEW
- **Description**: Every screen-sized GPU image in the renderer is built by the
  same five-step sequence — `vk::ImageCreateInfo` (TYPE_2D, 1 mip, 1 layer,
  `SampleCountFlags::TYPE_1`, `OPTIMAL`, `EXCLUSIVE`, `UNDEFINED`) →
  `device.create_image` → `allocator.lock().allocate(AllocationCreateDesc {
  location: GpuOnly, linear: false, allocation_scheme: GpuAllocatorManaged })`
  → `bind_image_memory` → `create_image_view(TYPE_2D, color_subresource_single_mip())`
  — plus the same three-arm error cleanup (destroy image on alloc failure; free
  alloc *then* destroy image on bind failure; same again on view failure). This
  is copied twelve times. `crates/renderer/src/vulkan/buffer.rs` solved the
  identical problem on the buffer side (`GpuBuffer::create_vertex_buffer` /
  `create_index_buffer` / `create_host_visible` / `create_host_readback` /
  `create_device_local_uninit` / `create_device_local_buffers_batched`, plus
  `StagingPool`/`StagingGuard`); the image side never got the analogue.
- **Evidence**:
  - `taa.rs::create_history_image` and `svgf.rs::create_history_image` are
    line-for-line the same ~85-line body, differing only in `HISTORY_FORMAT`
    (a const) vs `format` (a parameter) — including the SAFETY comments,
    the `with_context(|| format!("create {name}"))` message shapes, and the
    order of `free(alloc)` before `destroy_image(image)` in every error arm.
  - 14 files call `bind_image_memory`; 12 of them are this chain (`buffer.rs`
    and `texture.rs` are the two legitimate specialisations).
  - The same defect class has now been fixed **four separate times, once per
    copy**, which is the cost this finding is really about:
    - #1163 — allocator `MutexGuard` held across an error arm that re-locks (`ssao.rs`, fixed in place);
    - #1164 — push-the-allocation-before-the-bind so the partial-state invariant is structural (`ssao.rs`);
    - #1165 — "deadlock identical to #1163" (`context/helpers.rs`);
    - #2178 / PERF-D3-03 — sub-allocation stranded on bind failure (`gbuffer.rs`), whose own comment reads *"Same shape as the sibling site in `frame_upscaler.rs::create_outputs` and the established pattern in `exposure.rs`"* — i.e. the author had to hand-check three copies.
    `svgf.rs` and `caustic.rs` each carry a comment saying *"Cf. ssao.rs for the
    #1163 separate-let pattern"* — twelve copies means twelve independent
    re-derivations of a lock-reentrancy rule.
- **Impact**: Every new render pass pays ~80 lines of ceremony and re-derives
  the cleanup ordering and the allocator-lock scope from scratch; every future
  fix to that ordering has twelve landing sites and no compiler help finding
  them. This is a Vulkan *resource-lifecycle* consolidation, not a
  render-pass/barrier one, so it is outside the
  `feedback_speculative_vulkan_fixes.md` caution — the behaviour is
  observable from `cargo test` via the existing `#[cfg(test)]` source-shape
  guards in `frame_upscaler.rs`.
- **Related**: #1163, #1164, #1165, #2178 (all CLOSED, all the same defect in
  different copies); `crates/renderer/src/vulkan/buffer.rs` (`GpuBuffer`) as the
  precedent.
- **Suggested Fix**: Add `GpuImage` next to `GpuBuffer` — either in
  `crates/renderer/src/vulkan/buffer.rs` (renaming it to a resources module) or
  a new `crates/renderer/src/vulkan/image.rs` — exposing
  `GpuImage::create_2d(device, allocator, name, extent, format, usage) -> Result<GpuImage>`
  with `image`/`view`/`allocation` fields, a `destroy(device, allocator)`, and
  the same `Drop` safety-net `GpuBuffer` has (#656). Migrate the twelve sites
  one file per commit; `gbuffer.rs` (`COLOR_ATTACHMENT | SAMPLED`) and
  `volumetrics.rs` (3D) need a `usage`/`image_type` parameter, not a second
  helper.
- **Effort**: medium (≤1 day) — decompose one file per commit

---

---

### TD2-2026-09-05-04: `ImageSpaceModifierFrame` and `ImageSpaceModifierView` are the same 14-field struct in two crates, joined by a hand-written field-by-field copy


- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - `crates/scripting/src/cinematic.rs` — `ImageSpaceModifierFrame` (+ its `impl Default`)
  - `crates/renderer/src/vulkan/presentation.rs` — `ImageSpaceModifierView` (+ its `impl Default`)
  - `byroredux/src/app_frame.rs` — the 14-line copy inside `.map_or_else(ImageSpaceModifierView::default, |state| { … })`
- **Status**: NEW
- **Description**: The two structs are identical field-for-field *and*
  default-for-default: `blur_radius_pixels`, `double_vision_strength`,
  `motion_blur_strength`, `radial_blur_strength`, `radial_blur_ramp_up`,
  `radial_blur_start`, `radial_blur_ramp_down`, `radial_blur_down_start`,
  `radial_blur_center: [f32; 2]`, `saturation`, `brightness`, `contrast`,
  `tint_color: [f32; 4]`, `fade_color: [f32; 4]`, with the same non-obvious
  identity defaults (`radial_blur_down_start: 1.0`,
  `radial_blur_center: [0.5, 0.5]`, `tint_color: [1.0, 1.0, 1.0, 0.0]`). They
  are bridged by an explicit 14-assignment literal in `app_frame.rs`. Nothing
  checks that the three stay in step.
- **Evidence**: both struct bodies and both `impl Default` bodies are
  reproducible side by side with no textual difference beyond the type name;
  `byroredux/src/app_frame.rs` spells out `blur_radius_pixels: frame.blur_radius_pixels`
  through `fade_color: frame.fade_color`. Adding a 15th IMAD field (the
  cinematic slice is M47.2 and still growing) requires three edits, and omitting
  the third is silent — the field just never reaches the GPU.
- **Impact**: The same lockstep-drift shape that
  `feedback_shader_struct_sync.md` documents for the GPU structs, one tier up
  and with no size assertion to catch it. Blast radius is the whole IMAD /
  cinematic post-process path.
- **Related**: #3327 (the IMAD channel work); `feedback_shader_struct_sync.md`.
- **Suggested Fix**: Hoist one definition into `crates/core` (both
  `byroredux-scripting` and `byroredux-renderer` already depend on
  `byroredux-core`, and neither depends on the other — verified against both
  `Cargo.toml`s), e.g. `crates/core/src/imagespace.rs`, and have both crates
  re-export it. The `app_frame.rs` copy then deletes outright. This is the same
  move `crates/core/src/ecs/components/water.rs` already made for the shared
  water components.
- **Effort**: small (≤2 h)

---

---

### TD2-2026-09-05-05: `FloatTarget` / `ColorTarget` are duplicated verbatim across the `nif` → `core` boundary, bridged by a 20-arm identity match


- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - `crates/core/src/animation/types.rs` — `FloatTarget` (13 variants), `ColorTarget` (7 variants)
  - `crates/nif/src/anim/types.rs` — `FloatTarget` (13 variants), `ColorTarget` (7 variants)
  - `byroredux/src/anim_convert.rs` — the closures `convert_float_target` and `convert_color_target`
- **Status**: NEW
- **Description**: Both enums have the same variants in the same order with the
  same payloads (`MorphWeight(u32)` included) and the same derive set
  (`Debug, Clone, Copy, PartialEq, Eq, Hash`). The bridge between them is a pure
  identity map — 13 + 7 arms of `na::FloatTarget::X => FloatTarget::X`. Unlike
  `KeyType` (whose NIF side is `blocks::interpolator::KeyType`, a genuine
  wire-format enum decoded from the file), these two are *already* the
  post-translation semantic vocabulary on both sides: `crates/nif/src/anim/channel.rs`
  maps the raw `operation` / `target_color` discriminators onto the NIF-side
  enum, so the second enum adds no translation, only a second place to add a
  variant.
- **Evidence**: `byroredux/src/anim_convert.rs` contains
  `na::FloatTarget::Alpha => FloatTarget::Alpha` … `na::FloatTarget::RefractionStrength => FloatTarget::RefractionStrength`
  and `na::ColorTarget::Diffuse => ColorTarget::Diffuse` …
  `na::ColorTarget::LightAmbient => ColorTarget::LightAmbient`, with no arm doing
  anything other than renaming the path. `crates/nif/Cargo.toml` already lists
  `byroredux-core = { workspace = true }`, so the re-export direction is
  available today. Adding `EmissiveMultiple` and `RefractionStrength` under
  #3327 required editing both enums and both match closures.
- **Impact**: Four edits per new animation-channel target instead of one, with
  the identity map as the only thing between a forgotten variant and a
  non-exhaustive-match compile error (which is at least loud) — but also 20
  lines of pure ceremony that read as if translation were happening. This is
  the *unconverged* sibling of the `crates/nif/src/anim/coord.rs` re-export the
  discovery recipe cites as the finished example.
- **Related**: #2304 (CLOSED, NIFAL-D7-03) — a different defect: that one
  covered the `operation`→`FloatTarget` / `target_color`→`ColorTarget`
  *discriminator tables* duplicated between the KF and embedded-animation arms
  *inside* `crates/nif`; this is the enum *type* duplicated across the crate
  boundary. Also `canonical_translation_layer.md` ("promote, don't add a third
  type").
- **Suggested Fix**: Make `crates/nif/src/anim/types.rs` re-export the core
  enums —
  `pub use byroredux_core::animation::types::{ColorTarget, FloatTarget};` —
  exactly as `crates/nif/src/anim/coord.rs` re-exports `byroredux_core::math::coord`,
  then delete `convert_float_target` / `convert_color_target` from
  `byroredux/src/anim_convert.rs`. Leave `KeyType` alone: its NIF side is a wire
  enum and its conversion is real.
- **Effort**: trivial (≤30 min)

---

---

### TD2-2026-09-05-06: `extensions.rs` repeats the guest-entry snapshot prologue ten times and the 11-field `DeliveryCommitContext` literal fourteen times


- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**: `byroredux/src/extensions.rs` — the prologue/epilogue pair inside
  `dispatch_console_command`, `invoke_owned_papyrus_provider`, and the
  `dispatch_*_inner` family (activate, cell-load, equipment, input-action,
  session, pending-custom-event, and two more), plus
  `struct DeliveryCommitContext<'a>` / `fn apply_delivery_result` at the bottom
  of the file
- **Status**: NEW
- **Description**: Every path that enters a sandboxed guest repeats the same
  block verbatim:
  `let principal = hosted.instance.principal().id().clone();` → read
  `self.principal_storage.values(&principal)` → `set_principal_storage_snapshot`
  → read `self.legacy_containers.get(&principal)` →
  `set_legacy_container_snapshot` → invoke the guest → `apply_delivery_result(…,
  DeliveryCommitContext { state, principal_storage, legacy_containers,
  pending_custom_events, pending_setting_writes, pending_actor_value_writes,
  pending_package_evaluations, pending_animation_commands,
  pending_reputation_writes, diagnostics, stats })`. The commit context is
  already a named struct and `apply_delivery_result` is already a free function
  — what was never factored is the *construction*, so the 11 `&mut self.<field>`
  borrows are typed out at every site.
- **Evidence**: 10 `set_principal_storage_snapshot` production call sites, each
  paired 1:1 with a `set_legacy_container_snapshot` (a perfectly symmetric
  10/10 in production; the extra hits are in the test module past ~line 6000).
  14 `DeliveryCommitContext { … }` literals × 11 fields ≈ 154 lines of pure
  borrow plumbing. `byroredux/src/extensions.rs` is ~5920 production LOC and is
  the largest file in the workspace — this is one of the reasons.
- **Impact**: A twelfth pending-command queue (the file already has six:
  custom events, setting writes, actor-value writes, package evaluations,
  animation commands, reputation writes — and the SDK surface is still growing)
  means editing fourteen call sites, and a site that forgets a field does not
  fail to compile if the field is later given a default. Named in the SKILL's
  own "young crates that have not yet seen a debt sweep" list.
- **Related**: `crates/mod-runtime` / `crates/sdk` (the crates this file
  adapts); Dim 1's `extensions.rs` finding (same file, different axis — Dim 1
  owns the file split, this owns the repeated block).
- **Suggested Fix**: Two moves inside `byroredux/src/extensions.rs`. (1) Group
  the nine non-`components` owned fields into a `struct DeliveryState` field on
  the host, so `let (components, delivery) = (&mut self.components, &mut self.delivery);`
  splits the borrow cleanly and `DeliveryCommitContext::new(delivery, &mut stats)`
  becomes one call. (2) Extract the prologue as a free
  `fn enter_guest(hosted: &mut HostedComponent, delivery: &DeliveryState) -> PrincipalId`
  — a free function, not a `&mut self` method, so the `hosted` borrow does not
  conflict. Each dispatch site then reads: bind entity, `enter_guest`, invoke,
  `apply_delivery_result`.
- **Effort**: medium (≤1 day)

---

---

### TD2-2026-09-05-07: 111 hand-written full-field `NifHeader` literals across 40 files, with ~18 rival local factory functions, while `NifHeader::detached` produces exactly that value


- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**: `crates/nif/src/header.rs` — `NifHeader::detached` (the
  consolidation site, 1 production caller);
  40 files under `crates/nif/src/` construct the literal by hand, among them
  `crates/nif/src/blocks/base.rs`, `crates/nif/src/blocks/skin.rs`,
  `crates/nif/src/blocks/texture.rs`, `crates/nif/src/blocks/multibound.rs`,
  `crates/nif/src/blocks/dispatch_tests/`, `crates/nif/src/blocks/shader_tests/`,
  `crates/nif/src/blocks/collision/`, `crates/nif/src/blocks/controller/`,
  `crates/nif/src/stream.rs`, `crates/nif/src/version.rs`
- **Status**: NEW
- **Description**: `NifHeader::detached(version, user_version, user_version_2)`
  builds exactly the twelve-field "minimal version context, every table empty"
  header that NIF block tests need — it was added for the M49 detached-CSG
  decode and has one production caller
  (`crates/nif/src/import/precombine.rs`). Every test fixture in the crate
  reimplements it: `grep -c "num_groups: 0,"` returns **111** across 40 files,
  and at least eighteen local factory functions wrap it under six different
  names (`header_at`, `make_header`, `test_header`, `make_header_fnv`,
  `make_header_fo4`, `make_header_oblivion`, `make_header_fo76`,
  `make_header_pre_oblivion_v10_2`, …), several of which are byte-identical
  across files (`base.rs` declares `header_at` twice in one file, and
  `dispatch_tests/legacy_particle.rs` + `dispatch_tests/controllers.rs` each
  declare a third and fourth identical copy).
- **Evidence**: the 14-line window scan over `crates/nif/src/blocks/` returns
  the `NifHeader { version, little_endian: true, user_version: 0, …,
  num_groups: 0 }` block as the single most-repeated fragment in the directory,
  6 occurrences across 5 files for one exact variant alone.
  `NifHeader::detached`'s body is that literal, field for field.
- **Impact**: Adding a thirteenth field to `NifHeader` breaks 111 sites instead
  of 1. (It is at least a compile error rather than silent — which is why this
  is LOW, not MEDIUM.) The secondary cost is that the six factory names make
  cross-file test reading harder than it needs to be.
- **Related**: #834 (the `Arc<str>` block-types change that already had to
  touch this many sites once).
- **Suggested Fix**: Migrate the fixtures to
  `NifHeader::detached(version, user_version, user_version_2)`, using
  `NifHeader { strings, max_string_length, ..NifHeader::detached(v, uv, uv2) }`
  for the handful that populate a string table. Then delete the per-file
  factories in favour of one `#[cfg(test)] pub(crate)` helper in
  `crates/nif/src/header.rs` for the recurring game presets (FNV / FO4 / FO76 /
  Oblivion). Mechanical and scriptable.
- **Effort**: small (≤2 h)

---

---

### TD2-2026-09-05-08: the `SubRecord` test-fixture builder is defined 32 times across `crates/plugin`, under three names and three incompatible signatures


- **Severity**: LOW
- **Dimension**: 2 — Logic Duplication
- **Location**: 29 files under `crates/plugin/src/` each declare their own; e.g.
  `crates/plugin/src/esm/records/common.rs`,
  `crates/plugin/src/esm/records/items.rs`,
  `crates/plugin/src/esm/records/actor/tests.rs`,
  `crates/plugin/src/esm/records/misc/world.rs` (three times in one file),
  `crates/plugin/src/esm/records/misc/quest.rs`,
  `crates/plugin/src/esm/records/misc/pack.rs`,
  `crates/plugin/src/esm/records/misc/water.rs`,
  `crates/plugin/src/esm/records/scol.rs` (twice),
  `crates/plugin/src/esm/records/movs.rs`, `.../outfit.rs`, `.../pkin.rs`,
  `.../soun.rs`, `.../list_record.rs`, `.../climate.rs`, `.../weather.rs`,
  `crates/plugin/src/esm/cell/support.rs`
- **Status**: NEW
- **Description**: Every ESM record test module opens by redeclaring the same
  two-line constructor. Three names are in circulation (`sub`, `mk_sub`,
  `make_sub`) across three signatures — `(&[u8; 4], &[u8])`,
  `(&[u8; 4], Vec<u8>)`, `([u8; 4], Vec<u8>)`, plus one
  `(&[u8; 4], impl Into<Vec<u8>>)` in `misc/scene.rs` — so a test moved between
  files does not compile. Six files additionally redeclare identical `edid()`
  and `modl()` zstring wrappers (`movs.rs`, `scol.rs`, `pkin.rs`, `outfit.rs`,
  `soun.rs`, `list_record.rs`), which the window scan reports as the single most
  duplicated fragment in `crates/plugin/src/esm/records/`.
- **Evidence**: 32 definitions matching
  `fn (mk_)?sub|fn make_sub` returning `SubRecord` across 29 files;
  116 `SubRecord { … }` literals in the workspace.
- **Impact**: Pure friction rather than risk — but it is the reason a new record
  parser's test module starts with boilerplate instead of a test, and the
  three-signature split actively discourages moving coverage between files.
  Reported here rather than under Dim 9 per the cross-dimension rule: the defect
  is duplication, not test quality.
- **Related**: #1631 (TD7-002, CLOSED — the CNTO-size duplication in the same
  tree); #2414 / #2068 (the production-side `CommonNamedFields` consolidations
  that already landed).
- **Suggested Fix**: Add `crates/plugin/src/esm/records/test_support.rs`, gated
  `#[cfg(test)]` and declared `pub(crate)`, holding one
  `sub(typ: &[u8; 4], data: impl Into<Vec<u8>>) -> SubRecord` (the widest of the
  existing signatures, so every current call site compiles unchanged) plus
  `edid(&str)` / `modl(&str)` / `full(&str)`. Delete the 32 local copies. The
  `#[cfg(test)] pub(crate)` shape is already used elsewhere in this workspace
  (`crates/nif/src/blocks/controller/tests.rs` exposes `pub(super)
  make_header_fnv`), so the pattern is established.
- **Effort**: small (≤2 h)

---

## Considered and dropped

- **`CellData` struct literal, 21 sites / 9 files** (`crates/plugin/src/esm/cell/mod.rs`,
  `byroredux/src/cell_loader/{exterior,lod_support}.rs`,
  `byroredux/src/scene/world_setup.rs`, `crates/plugin/src/esm/cell/{walkers,wrld}.rs`) —
  real, but only ~4 of the copies are the full 27-field form, and the fix is a
  one-line `#[derive(Default)]` on `CellData` plus `..Default::default()` at the
  fixtures. Too small to carry a finding of its own; fold into whichever issue
  next touches that file.
- **`enum Archive { Bsa(BsaArchive), Ba2(Ba2Archive) }`, 3 copies across
  `crates/scripting/examples/fragment_coverage.rs`,
  `crates/pex/examples/pex_corpus_smoke.rs`,
  `crates/pex/examples/pex_corpus_shapes.rs`** — duplicates
  `byroredux/src/asset_provider/archive.rs::GameArchive`, but example binaries
  cannot depend on the `byroredux` binary crate, so consolidating means
  promoting the wrapper into `crates/bsa` — a public-API decision, not a debt
  cleanup. Noted, not filed.
- **Compute descriptor-set-layout binding arrays** (`taa.rs`, `svgf.rs` ×2,
  `caustic.rs`, `ssao.rs`) — N consecutive `COMBINED_IMAGE_SAMPLER` /
  `ShaderStageFlags::COMPUTE` bindings spelled out per pass. `DescriptorSetBuilder::from_layout_bindings`
  already consumes them; only the array construction repeats, and the binding
  *indices* are genuinely per-pass. Not worth a helper.
- **`snapshot<T>` / `drain<T>` in `crates/scripting/src/{dialogue,package}.rs`** —
  two copies of a generic component-snapshot pair. Two sites, ~15 lines, no
  divergence; below the reporting bar.

---

### TD3-2026-09-05-02: today's `fa5c4191` renamed `compute_blas_budget` → `probe_blas_heap_bytes` and changed its formula; four doc sites still describe the old name and the old `heap / 3` math


- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**:
  - `crates/renderer/src/vulkan/acceleration/mod.rs` — the `blas_budget_bytes` field doc (name + formula + lifecycle)
  - `crates/renderer/src/vulkan/acceleration/constants.rs` — the `MIN_BLAS_BUDGET_BYTES` doc (name + formula)
  - `crates/renderer/src/vulkan/acceleration/tests/predicates_tests.rs` — inside `should_evict_mid_batch`'s zero-budget case (name only)
  - `docs/engine/memory-budget.md:406` — the `MIN_BLAS_BUDGET_BYTES` reserve-floor table row (formula only)
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: `fa5c4191` (2026-09-05, **today** — *"Fix #3829, fix #3839, fix #3840"*).
- **Description**: `fa5c4191` split the old composed `compute_blas_budget` into `probe_blas_heap_bytes` (device probe) + `blas_budget_for_heap(heap, reserved)` (the arithmetic), and changed the arithmetic from `heap / 3` to `(heap - reserved) / 3` where `reserved` is the resolution-scaled froxel/pass reservation, re-derived on every swapchain recreate. Four doc sites still describe the pre-`fa5c4191` world. The exact stale sentences:
  1. `mod.rs`, `blas_budget_bytes` field doc:
     *"Derived **at construction time** from DEVICE_LOCAL heap size (**VRAM / 3**) with a 256 MB floor. On a 12 GB GPU this **yields 4 GB** (eviction virtually never fires); on a 6 GB GPU it **yields 2 GB**"* — followed by a rustdoc intra-doc link
     ``[`compute_blas_budget`](super::predicates::compute_blas_budget)``.
     All four claims are now wrong: the budget is *not* fixed at construction (its own sibling field `blas_heap_bytes`, added in the same commit, says *"Retained so [`Self::recompute_blas_budget`] can re-derive against a new screen-scaled reservation on resize"*), the formula subtracts a reservation first, the two worked GPU examples no longer hold, and the link target does not exist.
  2. `constants.rs`, `MIN_BLAS_BUDGET_BYTES` doc: *"Computed budget is **`device_local / 3`** capped no lower than this … See **`compute_blas_budget`**."*
  3. `predicates_tests.rs`: *"degenerate configuration; **`compute_blas_budget`** floors at 256 MB"* — the 256 MB floor claim is still true; only the function name is dead.
  4. `docs/engine/memory-budget.md:406`: `` | `MIN_BLAS_BUDGET_BYTES` | 256 MB | Minimum BLAS-budget floor (**BLAS allocation heap / 3**, capped below) | ``
- **Evidence**:
  ```
  $ git show fa5c4191 -- .../acceleration/predicates.rs | grep -E "^[-+].*fn (compute_blas_budget|probe_blas_heap_bytes|blas_budget_for_heap)"
  -pub(super) fn blas_budget_for_heap(heap_bytes: vk::DeviceSize) -> vk::DeviceSize {
  +pub(super) fn blas_budget_for_heap(
  -pub(super) fn compute_blas_budget(
  +pub(super) fn probe_blas_heap_bytes(
  ```
  Live arithmetic (`predicates.rs`):
  ```rust
  pub(super) fn blas_budget_for_heap(heap_bytes, reserved_bytes) -> vk::DeviceSize {
      (heap_bytes.saturating_sub(reserved_bytes) / 3).max(MIN_BLAS_BUDGET_BYTES)
  }
  ```
  `grep -rn "compute_blas_budget"` over `crates/` returns only doc-comment/test-comment hits plus the unrelated *method* `AccelerationManager::recompute_blas_budget` (`memory.rs`), which does exist.
- **Impact**: The `mod.rs` site is a rustdoc **intra-doc link to a non-existent path**, so `cargo doc` will emit a `broken_intra_doc_links` warning and the rendered docs get a dead reference. Beyond the build noise, this is the fifth recorded instance of the BLAS-budget doc block drifting from its code (#1625 closed a skill citing a non-existent `predicates.rs::blas_budget_bytes` *function*; #3043 corrected the heap-selection prose; #3842 was filed **today** against the sibling orphaned doc comment inside `predicates.rs` itself). The `mod.rs` block is now internally self-contradicting between two adjacent fields written by the same commit, which is the state most likely to send the next reader or auditor to the wrong conclusion about when the budget is computed.
- **Related**: #3842 (filed today — the *orphaned* `compute_blas_budget` doc comment inside `predicates.rs`, and its surviving "`VRAM / 3`" phrasing; **a different file and a different doc block** — the four sites above are not covered by it), #3043, #1625, #3839.
- **Suggested Fix**: Rewrite the `mod.rs` `blas_budget_bytes` doc to say the budget is `(probed DEVICE_LOCAL heap − screen-scaled pass reservation) / 3` floored at `MIN_BLAS_BUDGET_BYTES`, re-derived by `recompute_blas_budget` at init and on every swapchain recreate; drop the two now-wrong 12 GB/6 GB worked examples or recompute them against a stated reservation; repoint the intra-doc link at ``[`blas_budget_for_heap`](super::predicates::blas_budget_for_heap)``. In `constants.rs` and `memory-budget.md:406`, change `device_local / 3` / `BLAS allocation heap / 3` to `(heap − reserved) / 3`. In `predicates_tests.rs`, swap the name to `blas_budget_for_heap`.

---

---

### TD3-2026-09-05-03: `docs/feature-matrix.md` says the CTDA evaluator covers "13 functions" and `npc-spawn-ai-packages.md` says "~15"; the live `ConditionFunction::CATALOG` holds 19


- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `docs/feature-matrix.md:172` · `docs/engine/npc-spawn-ai-packages.md:134`
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: the "13" was itself the fix for #1818 (*"correct CTDA condition function count in feature-matrix.md (7 → 13)"*, commit `1d3190fb`). The catalog has since grown in at least three commits (`e9aece79` → `6df3bad8` → `583a349a`) with no matrix update.
- **Description**: The Scripting (M47) table row reads
  `| CTDA condition evaluation with OR-precedence (M47.1) | ✓ **13 functions** |`.
  The `npc-spawn-ai-packages.md` fail-open paragraph reads *"the M47.1 catalog covers **~15** of Bethesda's ~300 condition functions"*. The live catalog is 19.
- **Evidence**: `crates/scripting/src/condition.rs`:
  ```rust
  pub const CATALOG: [ConditionFunction; 19] = [
      Self::GetDistance, Self::GetActorValue, Self::GetDead, Self::GetStage,
      Self::GetStageDone, Self::GetInCell, Self::GetIsClass, Self::GetIsRace,
      Self::GetIsID, Self::GetFactionRank, Self::GetLevel, Self::GetEquipped,
      Self::HasPerk, Self::GetXPForNextLevel, Self::IsSceneActionComplete,
      Self::HasLoaded3D, Self::GetReputation, Self::GetReputationThreshold,
      Self::GetVMScriptVariable,
  ];
  ```
  The `ConditionFunction` enum has the matching 19 variants. The six additions past 13 —
  `HasPerk`, `GetXPForNextLevel`, `IsSceneActionComplete`, `HasLoaded3D`,
  `GetReputation`/`GetReputationThreshold`/`GetVMScriptVariable` — span the CHARAL,
  SCEN-playback and two-state-activator work.
- **Impact**: `docs/feature-matrix.md` is named in `_audit-common.md`'s Key Reference Docs table as the authority for *"what works at runtime per game"*, and the shared protocol tells auditors to *prefer these docs over re-deriving facts from source*. An auditor sizing the M47.1 gap (or a `/audit-scripting` dimension counting catalog coverage) reads 13 and under-counts by 32%. `docs/audits/AUDIT_SCRIPTING_2026-08-03.md` already shows the drift propagating — it says *"matching the 13 previously-verified catalog functions"* while adding six more in the same sentence. This is the same recurrence pattern as #2417/#2416/#2309/#2253/#2192/#2047, all CLOSED feature-matrix drift.
- **Related**: #1818 (the prior correction of this exact cell, 7→13), #2975, #2417, #2416.
- **Suggested Fix**: `✓ 13 functions` → `✓ 19 functions`; `~15 of Bethesda's ~300` → `19 of Bethesda's ~300`. Better: make the count a one-line assertion — `ConditionFunction::CATALOG.len()` is already a `const`, so a test asserting the documented figure would stop the third recurrence.

---

---

### TD3-2026-09-05-04: `triangle.frag` describes R1 Phase 6 as still pending in three present-tense comments, and attributes the UV/alpha identity defaults to `GpuInstance::default()` — a struct that has carried none of those fields since 2026-05-01


- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `crates/renderer/shaders/triangle.frag:213-218`, `:226-231`, `:376-384`
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: R1 (MaterialTable refactor) closed **2026-05-01** across 6 phases (`aa48d64`..`22f294a`) per `ROADMAP.md`. **~4 months stale.**
- **Description**: Three comment blocks in the main fragment shader narrate the R1 migration as in-flight. The exact stale sentences:
  1. `:216-218` — *"The legacy per-instance copies on `GpuInstance` **are still populated** by the CPU pipeline **(Phase 6 drops them)** and are byte-equal to `mat.*`, so the visible output is unchanged."*
  2. `:379-383` — *"The per-instance `inst.roughness` slot **is still populated** by the CPU pipeline (Phase 6 drops it once every reader has migrated); the value at `materials[inst.materialId].roughness` is byte-equal to it **for now** … **Phases 5 and 6 migrate the remaining per-material fields one slice at a time.**"*
  3. `:228-230` — *"Identity defaults (offset=(0,0), scale=(1,1), alpha=1.0) come from **`GpuInstance::default()`**"* — attached directly above a line that reads `mat.uvScaleU` / `mat.uvOffsetU`.
- **Evidence**: The live `GpuInstance` (`scene_buffer/gpu_types.rs`) is `model`, `texture_index`, `bone_offset`, `vertex_offset`, `index_offset`, `vertex_count`, `flags`, `material_id`, `ior`, `avg_albedo_r/g/b`, `surface_id`, `skinned_vertex_address`, `_reserved`, `morph_delta_address`, `morph_weight_address`, `morph_target_count`, `_reserved2a/b/c`. There is **no** `roughness`, `metalness`, `emissive_*`, `specular_*`, `alpha_threshold`, `uv_offset_*`, `uv_scale_*` or `material_alpha` field — Phase 6 completed. The identity defaults the comment attributes to `GpuInstance::default()` actually live in `impl Default for GpuMaterial` (`crates/renderer/src/vulkan/material.rs`):
  ```rust
  material_alpha: 1.0,
  uv_offset_u: 0.0, uv_offset_v: 0.0,
  uv_scale_u: 1.0,  uv_scale_v: 1.0,
  ```
  `GpuInstance`'s own `Default` impl contains no `uv_*` or `alpha` field at all.
- **Impact**: Anyone reading the shader to understand where a per-material value comes from is told two mutually reinforcing falsehoods: that a redundant per-instance copy exists and is authoritative-equal, and that the correct place to look for a default is `GpuInstance`. Both claims would send a contributor hunting for fields on the wrong struct — the same confusion `/audit-renderer`'s recurring "#785 trap" (`ui.vert` reading `textureIndex` not `materialId`) exists to guard against. `docs/audits/AUDIT_RENDERER_2026-05-01.md` (R1-N1) already documents the retained-field exceptions as a live hazard; leaving the shader claiming *more* retentions than exist compounds it. No runtime effect — `mat.*` is what the shader actually reads.
- **Related**: `ROADMAP.md` R1 row (closed 2026-05-01), #785, #2045 (CLOSED — a different `triangle.frag` doc/constant defect).
- **Suggested Fix**: Reframe both `:216-218` and `:379-383` in the past tense — R1 Phases 4-6 are closed and `GpuInstance` no longer carries per-material copies; delete the "byte-equal … for now" and "Phases 5 and 6 migrate …" clauses. At `:229`, change `` `GpuInstance::default()` `` → `` `GpuMaterial::default()` ``.

---

---

### TD3-2026-09-05-05: `legacy_pbr_translation_tests.rs`'s module doc still names the deleted `Material::classify_pbr` as a live sharing partner — the site #1624's own SIBLING completeness check was meant to catch


- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `crates/nif/src/import/material/legacy_pbr_translation_tests.rs:7-8`
- **Status**: Same class as CLOSED #1624 / #1522 / #1321 — **new site**
- **Effort**: trivial (≤30 min)
- **Age**: the file was created 2026-05-25 (`7fe85158`). #1624's fix landed 2026-06-15 (`eb9f6983`), three weeks later, and repaired only the sibling doc in `import/material/mod.rs`. **~3.5 months stale, and it was already in the tree when the sweep that should have found it ran.**
- **Description**: The module docstring reads:
  > *"The classifier itself **is shared with `Material::classify_pbr`** via `byroredux_core::ecs::components::material::classify_pbr_keyword`, so the heavy keyword-arm coverage lives next to that function in the core crate."*

  `Material::classify_pbr` was deleted in the NIFAL refactor — PBR resolves once at the parse-time `translate_material` boundary. The present-tense "is shared with" asserts a live render-time consumer that does not exist. (The sentence does then name the real free function, which is why this is LOW rather than the outright-misdirection of the earlier sites.)
- **Evidence**:
  ```
  $ grep -rn "fn classify_pbr\b" --include='*.rs' .        # zero hits
  $ grep -n "classify_pbr" crates/core/src/ecs/components/material.rs
  828: /// `Material::classify_pbr` (the per-draw fallback that was removed in
  1235: ///  glossiness-fallback in the (deleted per-draw) `classify_pbr`
  1417: /// the hard-coded lists in the (deleted) `Material::classify_pbr`
  1486: /// fields, the way the deleted `Material::classify_pbr` used to
  1008: pub fn classify_pbr_keyword(inputs: PbrClassifierInputs<'_>) -> PbrMaterial {
  ```
  The canonical file the skill points at is **clean** — every mention there is explicitly `(deleted)`. The surviving live-framing is only in this NIF-side test module. `crates/spt/src/import/mod.rs` also mentions `classify_pbr_keyword`, correctly, by its live name.
- **Impact**: This is the fourth site of a defect class already closed three times (#1321, #1522, #1624). Its consequence is a NIFAL-invariant misread: a reader concludes a render-time PBR classifier still exists, contradicting `docs/engine/nifal.md`'s "resolve-once at the translate boundary / no-render-time-fallback" rule — the rule whose violation `_audit-severity.md` scores HIGH. `feedback_audit_findings.md` records that ~5 of 30 findings in a past sweep had stale premises; a doc asserting a deleted classifier is live is precisely how such a premise is manufactured.
- **Related**: #1624 (whose completeness check read *"SIBLING: No other doc names the deleted `Material::classify_pbr` as live"* — this file falsifies that check), #1522, #1321.
- **Suggested Fix**: Rewrite to *"The classifier itself is the free function `byroredux_core::ecs::components::material::classify_pbr_keyword`, shared by the parser-side and canonical-translation paths; the render-time `Material::classify_pbr` it once mirrored was removed in the NIFAL refactor."* While there, run `grep -rn "Material::classify_pbr"` across the whole tree and confirm every remaining mention carries "deleted"/"removed" — this is the recurrence that keeps escaping single-file fixes.

---

---

### TD3-2026-09-05-06: `CLAUDE.md`'s Quick Reference says `cargo test -p byroredux-core` runs "162 tests"; the crate carries 746 `#[test]` functions


- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `CLAUDE.md:10`
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: `627b794a` (**2026-04-09**, *"docs: session 6 — N26 closeout"*). **~5 months stale**, never touched since.
- **Description**: The Quick Reference block reads:
  ```
  cargo test -p byroredux-core    # Run ECS/core tests (162 tests)
  ```
  `crates/core` (crate name confirmed `byroredux-core` in its `Cargo.toml`) now holds **746** `#[test]` functions — a 4.6× undercount. `ROADMAP.md`'s own last-verified line puts the whole workspace at 7 185 tests as of 2026-09-03, so the figure is stale against the project's own maintained counter too.
- **Evidence**:
  ```
  $ grep -rn "#\[test\]" --include='*.rs' crates/core | wc -l
  746
  $ grep -n "^name" crates/core/Cargo.toml
  2:name = "byroredux-core"
  $ git log --oneline -S"162 tests" -- CLAUDE.md | tail -1
  627b794a  (2026-04-09)
  ```
- **Impact**: `CLAUDE.md` is loaded at the start of every session by this project's own tooling — the single most-read document in the repo — and its per-command annotations are the first sizing signal an agent or contributor gets. A 4.6× wrong count invites "did I break something?" on a normal run and gives a false baseline to anyone estimating core-crate coverage. This is the identical defect class as TD3-NEW-01 in `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (CLAUDE.md's `Vertex` stated 100 B when a test had pinned 104 B) — that one was found and fixed; the neighbouring line eight rows above it was not checked.
- **Related**: TD3-NEW-01 (`AUDIT_TECH_DEBT_2026-07-25.md` — CLAUDE.md `Vertex` 100 B → 104 B), `ROADMAP.md:15` (the maintained workspace test counter).
- **Suggested Fix**: Either update to the live figure, or — preferably, since it will rot again within a session — drop the parenthetical entirely and let `ROADMAP.md`'s session-close-refreshed counter be the single source of truth for test totals, matching CLAUDE.md's own stated policy: *"Authoritative sources — do not duplicate state into this file."* The `162 tests` annotation is exactly the duplicated state that policy forbids.

---

## Summary

| ID | Severity | Effort | Subject |
|---|---|---|---|
| TD3-2026-09-05-01 | **MEDIUM** | trivial | `bindings.glsl` `GpuMaterial` 396 B + non-existent `_396_bytes` test (**regression of #1755**) |
| TD3-2026-09-05-02 | LOW | trivial | 4 sites naming the day-old-deleted `compute_blas_budget` + its pre-`fa5c4191` `heap / 3` formula |
| TD3-2026-09-05-03 | LOW | trivial | feature-matrix "13 functions" / npc-spawn "~15" vs live `CATALOG` of 19 |
| TD3-2026-09-05-04 | LOW | trivial | `triangle.frag` narrates closed R1 Phase 6 as pending; wrong default-source struct |
| TD3-2026-09-05-05 | LOW | trivial | `Material::classify_pbr` framed live in a NIF test module (4th site of a 3×-closed class) |
| TD3-2026-09-05-06 | LOW | trivial | `CLAUDE.md` "162 tests" vs 746 (5 months stale, in the most-read file in the repo) |

**Total: 6 findings — 1 MEDIUM, 5 LOW. All trivial effort (< 3 h combined).**

Publish under `doc-rot` + `documentation` (not `bug`), plus domain: `shaders`/`renderer`
for -01 and -04, `renderer`/`memory` for -02, `scripting` for -03, `nifal`/`nif` for -05,
`tech-debt` for -06.

**Recurrence signal worth surfacing at merge**: three of the six (-01, -03, -05) are
re-occurrences of defects already closed at least once in the same file or the same
sentence. Point fixes are not holding on the numeric-claim class. Two structural options
worth one issue of their own: emit `GPU_MATERIAL_SIZE_BYTES` from `build.rs` into
`shader_constants.glsl` (kills -01 permanently, using infrastructure that already exists),
and assert documented catalog counts against `CATALOG.len()` (kills -03).

---

### TD4-2026-09-05-02: six more LOC figures in `_audit-common.md`'s Binary / Gameplay rows are stale — one by 98%, one contradicted by this audit's own skill file


- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/_audit-common.md:70`, `:80`, `:84`, `:85`, `:86`, `:89`
- **Status**: NEW
- **Effort**: trivial (≤30 min)

**Description**
Every hand-typed LOC figure in the Binary-modules and Gameplay-slice rows has drifted:

| Row | Claimed | Live (`wc -l`) | Drift |
|---|---|---|---|
| `:70` `byroredux/src/main.rs` | 1053 | **1267** | +20% |
| `:80` `byroredux/src/interaction.rs` | 1493 | **1626** | +9% |
| `:84` `byroredux/src/combat.rs` | 952 | **1284** | +35% |
| `:85` `byroredux/src/inventory.rs` | 1008 | **1096** | +9% |
| `:86` `byroredux/src/settings_io.rs` | 345 | **7** | **−98%** |
| `:81` `byroredux/src/studio_host.rs` | 252 | **402** | +60% (folded into TD4-…-01) |

Two are more than drift:

1. **`settings_io.rs` is a 7-line re-export shim**, not a 345-LOC module. The
   body moved to `crates/settings-io` in `e05b4a9f` (2026-08-30, *"Share one
   settings model between the launcher and the engine"*):
   ```rust
   //! Settings persistence, re-exported from `byroredux-settings-io`.
   pub(crate) use byroredux_settings_io::{load, save, SettingsPersistence, SETTINGS_PATH_ENV};
   ```
   The row still describes it as *"settings persistence behind the game menu"*
   and the Gameplay-slice header still counts it in the *"~3.8k LOC landed from
   2026-08-15 on"* total. An auditor told to audit the gameplay slice's settings
   persistence opens a shim; the real code sits in a crate on the **un-owned**
   list (`crates/settings-io`, "No owner").

2. **`main.rs`'s 1053 is contradicted inside the audit corpus itself.** This
   audit's own `audit-tech-debt/SKILL.md:239` reads *"main.rs is 1267 LOC
   (re-measured 2026-09-05, up from 1053 …)"*. The two files disagree, and the
   newer one names the older number as the superseded one.

3. **`:89` calls `byroredux/src/scene.rs` "(thin)"** — it is 1,706 total /
   **1,646 production** LOC, the third-largest module in the binary. "Thin"
   was accurate when `scene/` was split out; it now steers auditors away from a
   file that is one growth spurt from Dim 1's primary bucket.

**Impact**
Documentation-only, but these rows are the sizing signal auditors use to
allocate attention, and one of them points at a shim while the real code sits
in an un-owned crate.

**Related**
TD4-2026-09-05-01 (same file, same defect class, promoted for demonstrated
misdirection). #3744 (CLOSED — prior consolidated skill-drift sweep; covered
semantic claims only, explicitly not LOC figures).

**Suggested Fix**
Re-measure all six. For `settings_io.rs`, rewrite the row to say the body moved
to `crates/settings-io` and point the Gameplay-slice reader there. Drop "(thin)"
from the `scene.rs` row. Longer term these literals want the #2420 treatment —
name the measuring command instead of the number.

---

---

### TD4-2026-09-05-03: two audit SKILL files still name `compute_blas_budget`, renamed hours ago by `fa5c4191`; one pins a stale line anchor, the other states the pre-#3839 formula


- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/audit-fnv/SKILL.md:84`, `.claude/commands/audit-performance/SKILL.md:122`
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: `fa5c4191`, 2026-09-05 (today) — fixed #3829/#3839/#3840 and split the function.

**Description**
`compute_blas_budget` no longer exists. `fa5c4191` split it into
`probe_blas_heap_bytes` (raw heap measurement) and `blas_budget_for_heap`
(heap → budget), so a resize can re-derive the budget without re-probing:

```rust
// crates/renderer/src/vulkan/acceleration/predicates.rs:742
pub(super) fn blas_budget_for_heap(heap_bytes: vk::DeviceSize, reserved_bytes: vk::DeviceSize) -> vk::DeviceSize {
    (heap_bytes.saturating_sub(reserved_bytes) / 3).max(MIN_BLAS_BUDGET_BYTES)
}
// :758
pub(super) fn probe_blas_heap_bytes(…) -> Result<vk::DeviceSize>
```

Both skill sites are wrong, each in an extra way beyond the name:

- **`audit-fnv/SKILL.md:84`** — ``predicates.rs::compute_blas_budget` =
  `device_local_bytes / 3` floored at `MIN_BLAS_BUDGET_BYTES``. The formula is
  also stale: #3839 added the `reserved_bytes` subtraction, so the live math is
  `(heap − reserved) / 3`, not `heap / 3`. (The row's other claim — that the
  result is cached in the `blas_budget_bytes` field of `acceleration/mod.rs` —
  is still **correct**; `mod.rs:205` carries it, alongside the new
  `blas_heap_bytes` at `:210`.)
- **`audit-performance/SKILL.md:122`** — lists `compute_blas_budget` **@707**
  under Dimension entry points. Line 707 is now inside
  `screen_scaled_reservation_bytes`, an unrelated function.

**Why the gate's symbol advisory did not catch either site** — two independent
blind spots, both verified:

1. **`audit-fnv:84` is invisible to the extractor.** It writes the symbol inside
   a longer backticked span, ``predicates.rs::compute_blas_budget``. Advisory
   pass 1 matches `` `<identifier>` `` (an exactly-one-identifier span) and pass 2
   matches `` `<SYMBOL> = `` ; a `path.rs::symbol` span matches neither, so the
   token is never even considered.

2. **`audit-performance:122` *is* extracted, then suppressed by stale mentions
   in the source it checks against.** The bare span ``compute_blas_budget``
   matches pass 1, but the existence test is `grep -qw "$sym" "$src_blob"` over
   concatenated tracked source — and the rename left three dead references
   behind:
   ```
   acceleration/mod.rs:203      /// [`compute_blas_budget`](super::predicates::compute_blas_budget)
   acceleration/constants.rs:60 /// footprint. See `compute_blas_budget`.
   tests/predicates_tests.rs:253  // configuration; `compute_blas_budget` floors at 256 MB so
   ```
   Any one of them satisfies `grep -w`, so the advisory concludes the symbol
   exists. (The six other `recompute_blas_budget` hits do **not** contribute —
   `grep -w` correctly rejects them; `AccelerationManager::recompute_blas_budget`
   at `memory.rs:459` is a real, live, differently-named method.)

Blind spot 2 is the more consequential of the two and is **self-reinforcing**:
doc rot in the code immunizes doc rot in the skills, so the two halves of the
same rename hide each other. It is a fourth entry in the family documented at
`_audit-validate.sh:236-249` — #3197's (a) SCREAMING_SNAKE_CASE and (b)
negative-assertion corpus hits, and #3052's (c) `SYMBOL = value` spans.
`acceleration/mod.rs:203` is additionally a **broken rustdoc intra-doc link**
(`super::predicates::compute_blas_budget` no longer resolves).

**Impact**
`grep compute_blas_budget` returns nothing in `crates/`, so an auditor following
either entry-point list lands nowhere and may conclude the BLAS-budget path was
deleted rather than renamed. The `audit-fnv` formula error is worse than a dead
name: it describes budget math that is quantitatively wrong post-#3839, and an
auditor could file a phantom finding against correct code.

**Related**
TD3-2026-09-05-02 (this audit, Dim 3 — the **non-skill** doc sites of the same
rename; skill sites are deliberately left here to avoid double-filing).
#3842 (OPEN, filed today — the orphaned `compute_blas_budget` **code** doc
comment). #3450 (CLOSED — prior instance of two skills pinning a renamed symbol).

**Suggested Fix**
Update both sites to the split pair and drop the `@707` anchor (line numbers
drift; the gate strips them from path checks for exactly this reason). Correct
`audit-fnv`'s formula to `(heap − reserved) / 3`. Two independent gate
hardenings follow from the analysis above, and only both together close it:
add a third extractor pass for the trailing identifier of a ``path.rs::symbol``
span (fixes 1), and restrict the corpus test to *definition* sites —
`fn <sym>` / `struct <sym>` / `const <sym>` / `let <sym>` — rather than any
whole-word hit, so a stale comment can no longer vouch for a deleted symbol
(fixes 2).

---

---

### TD4-2026-09-05-04: ~13 backtick-convention violations in the docs advisory — deliberately-absent, forward-looking and deleted names asserted as existing, one of them self-contradictory


- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `docs/engine/nifal.md:228`, `:486`; `docs/engine/exal.md:222`, `:240`, `:614`; `docs/engine/ui.md:376`; `docs/engine/ecs.md:210`; `docs/engine/game-loop.md:173`; `docs/engine/starfield-esm-roadmap.md:122`; `docs/engine/cxx-interop.md:12`; `docs/engine/watal.md:567`; `docs/engine/sandboxed-linked-mods.md:292`
- **Status**: NEW
- **Effort**: trivial (≤30 min)

**Description**
Triage of the gate's 240-symbol `docs/engine` advisory. The convention
(`_audit-common.md:277-286`) is that a backticked symbol **asserts it exists
right now**; historical, deleted, or not-yet-built names must be *italicised*.
Filtering the 240 for negation / forward-looking context yields these genuine
violations — every one is a name the sentence itself says does not exist,
carried in backticks that say it does:

| Site | Text | Class |
|---|---|---|
| `nifal.md:228` | "there is no single `translate_node` boundary to collapse them into" | never-built |
| `exal.md:614` | "No separate `translate_sun`/`SunModel`" | never-built (×2 symbols) |
| `exal.md:222` | "A **future** `translate_sun` (step 4) will fold…" | forward-looking |
| `ui.md:376` | "There is no bespoke `update_ui_texture` entry point" | never-built |
| `ecs.md:210` | "There is no `query_3_mut`/`query_4_mut`" | never-built (×2) |
| `game-loop.md:173` | "There is **no standalone `input_system`**" | never-built |
| `starfield-esm-roadmap.md:122` | "New test `parse_cydonia_cell` … **proposed, does not exist yet**" | forward-looking (self-declared!) |
| `cxx-interop.md:12` | "The unused Rust→C++ `EngineInfo` export **was removed**" | deleted |
| `nifal.md:486` | "`PrecombineMaterial` subset and field-by-field patch operation **were removed**" | deleted |
| `watal.md:567` | "does not introduce a second `WaterLod` material representation" | never-built |
| `sandboxed-linked-mods.md:292` | "The eventual schema name is deliberately left open. It represents a `ModManifest`" | forward-looking (lower confidence — sentence self-qualifies) |

**One is content rot, not just convention.** `SunModel` exists in no `.rs`
file (`grep -rn SunModel crates/ byroredux/ --include='*.rs'` → 0 hits), yet
`exal.md` names it **both ways**:

```
exal.md:240   the canonical `WeatherDataRes` + `SunModel`, not a translate site.   ← asserts it EXISTS
exal.md:614   No separate `translate_sun`/`SunModel`                               ← asserts it does NOT
```

`WeatherDataRes` does exist (`byroredux/src/env_translate.rs`, `cornell.rs`,
`boot.rs`). So `:240` pairs a real canonical resource with a phantom one and
presents the pair as the thing `weather_system` samples.

Excluded as false positives after checking context: `MyActorScript`
(`scripting.md:768` — a pedagogical placeholder in a worked example, not a repo
claim), `GreaterThan` (Papyrus event vocabulary), and the ~225 remaining
advisory entries, which are the documented noise floor (GMST/perk/actor-value
rosters, nif.xml field names, on-disk format fields, Vulkan entry points).

**Impact**
Each violation is a name an auditor can `grep` for, fail to find, and file as
missing/deleted — the exact false-finding loop the convention exists to stop.
`exal.md:240` is worse: it can produce a finding that the "canonical `SunModel`"
is unimplemented, against a design that never intended one.

**Related**
#3197 (CLOSED — the advisory's two prior structural blind spots; this is the
first triage pass since the advisory started reporting a non-zero docs list).
#3052 (CLOSED — an audit skill naming a backticked symbol that exists nowhere,
same defect one tier up). Dim 3 of this audit spot-checked these and routed the
triage here rather than filing; no overlap with TD3-2026-09-05-01…06.

**Suggested Fix**
Italicise all of them (`*translate_node*`, `*translate_sun*`, …) per the
convention. Fix `exal.md:240` separately — it is a content error, not a
formatting one: either drop `SunModel` from the sentence or mark it as the
not-yet-built model `:614` says it is.

---

---

### TD4-2026-09-05-05: seven dead backticked bare basenames sit in the skill tier itself, invisible to the gate per #3439 — and one of them makes `audit-incremental` state a fact that is wrong for three of its four names


- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/_audit-common.md:90`; `.claude/commands/audit-renderer/SKILL.md:34`, `:209`; `.claude/commands/audit-fo4/SKILL.md:157`, `:160`; `.claude/commands/audit-oblivion/SKILL.md:188`; `.claude/commands/audit-incremental/SKILL.md:96`
- **Status**: NEW (the *content* backlog; the gate gap itself is **Existing: #3439**, not re-filed)
- **Effort**: trivial (≤30 min)

**Description**
**#3439's premise re-verified and it still holds**, with one correction worth
recording: the issue's headline evidence was `grep -c '`ai\.rs' docs/engine/npc-spawn-ai-packages.md` → **10**; today that file carries **1**. The
*structural* gap is unchanged — `should_skip()`'s first rule still discards
every ref without a `/`, and `git ls-files | grep -E "(^|/)ai\.rs$"` still
returns nothing, so the surviving citation still passes silently. Do not treat
the shrunken count as the issue being fixed; the blind spot is intact.

Enumerating what that blind spot hides **inside my own scope** (the skill tier,
which #3439's evidence did not measure) finds seven dead refs:

| Site | Ref | Reality |
|---|---|---|
| `_audit-common.md:90` | ``fire_lights.rs`` | Deleted `2325c1de` (2026-08-17) — and the sentence **says so**: *"**`fire_lights.rs` was deleted 2026-08-17**"* |
| `audit-renderer/SKILL.md:209` | ``references.rs`` | Split into `byroredux/src/cell_loader/references/` (a directory: `attach.rs`, `complete.rs`, `import.rs`, `synth_child.rs`, `mod.rs`); no `references.rs` exists anywhere |
| `audit-fo4/SKILL.md:157`, `:160` | ``references.rs`` ×2 | same |
| `audit-renderer/SKILL.md:34` | ``render.rs`` | Deleted; the note is *"`byroredux/src/render/` is a **directory**, not a `render.rs` file"* |
| `audit-incremental/SKILL.md:96` | ``render.rs`` | same |
| `audit-oblivion/SKILL.md:188` | ``actor.rs`` | *"split from `actor.rs`, #2055"* — backwards-looking |

Two distinct convention faults are mixed here. `_audit-common.md:90`,
`audit-renderer:34` and `audit-oblivion:188` are **negative/backwards-looking
sentences that backtick the absent name** — the same fault as
TD4-2026-09-05-04, one tier up. `_audit-common.md:90` is the sharpest instance:
the same sentence correctly italicises the three deleted *symbols*
(*derive_fire_light* / *fire_lights_enabled* / *BYRO_FIRE_LIGHTS*) and then
backticks the deleted *file*. The convention was known and applied unevenly
within one sentence.

The three ``references.rs`` sites are the other fault — **positive assertions
about a file that no longer exists**:

```
audit-fo4/SKILL.md:157   `references.rs` fires the first matching expander and composes placements.
audit-fo4/SKILL.md:160   (`cell_loader/spawn.rs` + `references.rs`, cached in `nif_import_registry.rs`)
audit-renderer/SKILL.md:209  `references.rs` `debug_assert!`s loaded-cell max |coord| stays under it
```

**`audit-incremental/SKILL.md:96` additionally states something false.** Its
sentence is *"`render.rs`, `systems.rs`, `scene.rs`, and `cell_loader.rs` are
all directories now"*. Three of the four are still real files with a directory
sibling:

```
ABSENT: byroredux/src/render.rs
EXISTS: byroredux/src/systems.rs      (54 LOC)
EXISTS: byroredux/src/scene.rs        (1706 LOC)
EXISTS: byroredux/src/cell_loader.rs  (580 LOC)
```

This is a "layout shifts that often surprise a delta audit" callout — the one
paragraph whose entire job is to stop an auditor being surprised by layout.

**Impact**
Low blast radius (documentation), but it lands on the gate's *own* corpus, in
the paragraph meant to prevent layout surprise, and it demonstrates that
#3439's hole is not confined to `docs/engine/` — the tier it was measured in.
Each dead ``references.rs`` sends an FO4/renderer auditor to a path that does
not resolve.

**Related**
**Existing: #3439** (OPEN — the gate blind spot; deliberately not re-filed).
#1189 (CLOSED — *12 stale `byroredux/src/render.rs` refs in 5 audit skill
files*; that sweep fixed the **qualified** form, and these bare-basename
survivors are exactly what the gate could not see afterwards).
#1229 (CLOSED — same pattern for `tri_shape.rs`).

**Suggested Fix**
Repoint each ``references.rs`` site at its *actual* home — they are three
different files, so a blanket rewrite to `references/` would be wrong twice:

| Site | Correct target |
|---|---|
| `audit-renderer:209` (RT precision ceiling) | `byroredux/src/cell_loader/references/complete.rs:82` holds the assert; the constant and `worldspace_extent_over_rt_ceiling` are exported from `references/mod.rs` and pinned in `references/import_tests.rs` |
| `audit-fo4:157` (SCOL/PKIN expanders) | **`byroredux/src/cell_loader/refr.rs`** — `expand_scol_placements` / `expand_pkin_placements` never moved into `references/` |
| `audit-fo4:160` (attach-graph materialization) | `byroredux/src/cell_loader/references/attach.rs`, with `spawn.rs` and `nif_import_registry.rs` as the other two links; the cited `cell_loader/attach_points_spawn_tests.rs` does exist |

Then italicise *fire_lights.rs*, *render.rs* and *actor.rs* at the four
backwards-looking sites, and rewrite `audit-incremental:96` to name only
`render.rs` as gone while describing `systems.rs` / `scene.rs` /
`cell_loader.rs` as thin dispatch files beside their directories. Landing
#3439's two-line fix afterwards keeps the class from returning.

---

---

### TD4-2026-09-05-06: 26 CRITICAL/HIGH findings across 12 pre-`/audit-publish` reports have no GitHub trace — the mandated `docs/audits/` dedup step returns false-NEW on already-fixed work


- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `docs/audits/` — 12 reports dated 2026-04-04 … 2026-06-02
- **Status**: NEW
- **Effort**: small (≤2 h)

**Description**
`_audit-common.md:299-309` makes dedup mandatory and step 3 is *"Scan
`docs/audits/` for prior reports covering the same issue"*, with the routing
rule *"If OPEN: skip. If CLOSED: verify fix… If no match: report as NEW."*
That rule assumes every report finding reached GitHub. For the pre-`/audit-publish`
cohort it did not.

Matching every ID'd CRITICAL/HIGH finding in reports older than 90 days
(< 2026-06-07) against all 3,730 repo issue titles:

```
pre-2026-06-07 reports w/ ID'd CRITICAL/HIGH findings: 50
ID'd CRITICAL/HIGH findings: 131   no GitHub title match: 26
```

The 26 cluster in 12 reports:

| Report | Untraced / total | Findings |
|---|---|---|
| `AUDIT_NIF_2026-04-11.md` | 6/6 | NIF-04-11-C1/C2/C3 (CRITICAL), H1/H2/H3 |
| `AUDIT_NIF_2026-04-04.md` | 3/4 | NIF-009 (CRITICAL), NIF-008, NIF-301 |
| `AUDIT_SAFETY_2026-04-05.md` | 3/3 | SAFE-01, SAFE-02, SAFE-03 |
| `AUDIT_FNV_2026-04-21.md` | 2/6 | FNV-ESM-4, FNV-ESM-6 |
| `AUDIT_FO3_2026-05-01.md` | 2/2 | FO3-4-01, FO3-4-02 |
| `AUDIT_FO4_2026-06-02.md` | 2/2 | FO4-D6-GAP-05, FO4-D8-NEW-01 |
| `AUDIT_POSITIONING_DECALS_2026-04-13.md` | 2/2 | PD-01, PD-02 |
| `AUDIT_RENDERER_2026-04-12c.md` | 2/3 | RL-01 (CRITICAL), RL-02 |
| + 4 more, 1 each | 4/4 | CONC-D2-NEW-01, PERF-04-11-H3, MEM-002, SK-D5-NEW-01 |

**Spot-check: 3 of 3 were fixed, silently.**

- **SAFE-01** *"`write_mapped` silently truncates data exceeding buffer size"* —
  `crates/renderer/src/vulkan/buffer.rs:1273` now logs
  `"write_mapped: data ({} bytes) exceeds buffer capacity ({} bytes) — truncating"`.
  No longer silent.
- **PD-02** *"APP_CULLED flag (0x20) not checked in NIF walker"* — now filtered
  import-side with a dedicated regression file
  `crates/nif/src/import/tests/app_culled_visibility.rs` (#3640).
- **NIF-04-11-H1** *"Property inheritance from parent `NiNode`s is not applied"* —
  `extract_material_info(scene, shape, inherited, &mut pool)` threads an
  `inherited: &[BlockRef]` list, with `alpha_flag_tests.rs` pinning the
  shape-intent-wins cascade (#1201).

So the failure mode is not "26 live CRITICAL bugs". It is that an auditor
executing the mandated dedup finds a CRITICAL in `docs/audits/`, finds no
issue, and concludes **NEW** — re-filing fixed work, or worse, re-deriving a
"regression" against code that never regressed. `RL-01` is the canonical
example: it is recorded in user memory as *"audits claiming RL-01 is unfixed
have a bad premise"*, which is a person having had to absorb this exact loop.

**Impact**
One of the two mandated dedup inputs is unreliable for ~22% of the corpus
(140 of 629 reports predate the workflow). Cost is auditor time and
false-NEW findings, not runtime behaviour.

**Related**
#3218 (CLOSED) and **#3504 (OPEN)** cover the *opposite* direction —
issue → commit citation. Neither covers report-finding → issue, so this is
adjacent, not duplicate. #3440 (CLOSED — a wrong baseline number inside a
`docs/audits/` report, the same "reports rot too" family).

**Suggested Fix**
Cheapest durable fix is a dated caveat in `_audit-common.md`'s Deduplication
section: *"Reports dated before 2026-06-07 predate `/audit-publish`; their
findings have no issue trace. For those, verify against **code**, not GitHub —
absence of an issue is not evidence the finding is open."* Optionally extend
`scripts/check-issue-traceability.sh` with a report→issue direction so the
backlog is measured rather than rediscovered.

---

## Verified clean (no findings — recorded so the next audit can diff)

| Check | Result |
|---|---|
| **GPU struct sizes across the skill corpus** | **All current.** `audit-renderer:115` (160 / 368 / 432 B), `audit-performance:130` (160 B), `audit-regression:149-150` (160 / 368 B), `audit-safety:257` (432 B) all match the live pins `gpu_instance_layout_tests.rs` (`GpuInstance` 160, `GpuCamera` 368, `GpuLight` 64) and `material_tests.rs::gpu_material_size_is_432_bytes`. `_audit-common.md:100`/`:284` and `_audit-validate.sh:100`/`:223` quote 300/348/352 B only as **history**, correctly framed. #3450's class has not returned. |
| **Dimension counts** | **All match.** `audit-suite:179` "9 dimensions" = `/audit-nifal`'s 9; `:188`/`:197` "23 dimensions" = `/audit-renderer`'s 23. Every cross-skill dimension anchor in `_audit-common.md` resolves: `/audit-safety` Dim 1 (FFI) and Dim 11 (mod-runtime, of 11), `/audit-renderer` Dim 15/22/23, `/audit-physics` Dim 6 (of 7), `/audit-scripting` Dim 8 (of 8), `/audit-concurrency` Dim 7 (of 7), `/audit-ui` Dim 7 (of 7), `/audit-ecs` 10 dimensions. |
| **Symbol-anchor refs** (`path.rs::symbol`) | **All 40 resolve.** Every backticked `…rs::symbol` in the skill corpus has a live definition — incl. `crates/audio/src/lib.rs::drain_pending_oneshots`, the example the skill names. |
| **"Existing: #NNN" callouts in skills** | **None exist.** No skill file carries the pattern, so there is nothing to reframe as a closed-state baseline. |
| **Crate roster coverage** | **28/28.** Gate's crate-count guard passes; all 28 crates are named in `_audit-common.md`; the un-owned table's "Eight subsystems" claim matches its eight rows. |
| **audit-skills symbol advisory** | 4 entries, **all false positives** (3 GitHub label names + 1 finding-format field). |
| **`_audit-validate.sh` path gate** | GREEN — 0 STALE across 2,450 refs / 102 files. |

---

## Notes for the merge phase

- **TD4-2026-09-05-03 uncovered two more `symbol_advisory` blind spots** — a
  `path.rs::symbol` span the extractor never considers, and (the important one)
  an existence test that accepts a *stale comment* as proof a deleted symbol
  exists, so a rename's code-side rot conceals its skill-side rot. Both are
  written up inside that finding rather than filed separately; if the merge
  prefers them standalone they belong with #3197 / #3052 as blind spots (d)
  and (e), and only fixing both closes the case that produced this finding.
- **`boot.toml` / `settings.toml` / `profiles.toml` / `mem.frag`** surface as dead
  bare basenames under a naive scan but are **not** findings: the first three are
  runtime-written config files (`crates/boot-request/src/lib.rs:42`
  `DEFAULT_FILE_NAME = "boot.toml"`), and `mem.frag` is a **debug console
  command**, not a shader. If #3439's fix lands, `should_skip()` will need the
  same data-file exemption it already grants `*.bsa` / `*.esm` / `*.ba2` / `*.nif`,
  or the gate will go red on four legitimate refs.
- Per instruction, `.claude/issues/<N>/ISSUE.md` "Status: Open" drift was **not**
  examined (dropped per TD10-001 / #1156).

---

### TD5-2026-09-05-01: Dim 5's discovery recipe never looks at `tools/` — 4 first-party workspace crates, 4 706 LOC, invisible to four consecutive audits

- **Severity**: LOW
- **Dimension**: 5 (Stale Markers)
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md` — the Dimension 5
  **Discovery** block (`grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux`)
- **Status**: NEW
- **Age**: the recipe's scope has been `crates byroredux` since the dimension
  was written; the gap *widened* on 2026-08-30 when `tools/byro-launcher` and
  `tools/byro-detect` landed (~2 weeks old at time of audit).
- **Effort**: trivial (≤30 min — one word in the grep + a `nifskope` exclusion)
- **Description**: The Dim 5 recipe greps `crates` and `byroredux`. It does not
  grep `tools/`, which holds **four first-party workspace members** —
  `tools/byro-dbg`, `tools/byro-launcher`, `tools/byro-detect`,
  `tools/texture-upscale` — 17 `.rs` files, 4 706 LOC. These are not vendored:
  all four are listed in the root `Cargo.toml` `[workspace] members` array.
  (`tools/nifskope` is correctly *not* a member and must stay excluded — it is
  vendored reference code, per `_audit-common.md`'s Tools row.)

  A live `// TODO` in the launcher's engine-supervision path or in
  `byro-detect`'s `libraryfolders.vdf` parser would be structurally invisible to
  this dimension, indefinitely.

  The second-order harm is a claim wider than its evidence: the 2026-08-30
  report states *"**Zero live TODO/FIXME/HACK markers in the entire codebase** —
  production and shaders"* and *"There is not one live marker in the codebase."*
  The grep behind that sentence covered `crates` + `byroredux` + shaders, not
  the whole codebase. The conclusion happens to be true — I verified `tools/` is
  marker-free today — but it was **lucky, not measured**, and the next auditor
  inherits an unqualified whole-codebase claim.
- **Evidence**:
  ```
  # root Cargo.toml [workspace] members — all four are first-party:
  "tools/byro-detect", "tools/byro-launcher", "tools/byro-dbg", "tools/texture-upscale"
  # tools/nifskope is absent from members → vendored, correctly out of scope

  $ find tools -name '*.rs' -not -path 'tools/nifskope/*' | wc -l   → 17
  $ find tools -name '*.rs' -not -path 'tools/nifskope/*' -exec wc -l {} + | tail -1
                                                                    → 4706 total
  $ grep -RInE '(TODO|FIXME|HACK|XXX)\b' tools --include='*.rs' --include='*.toml' \
      | grep -v nifskope                                            → (empty)
  ```
- **Impact**: No live debt is hidden today (verified). The blast radius is
  future-blindness over the exact code `_audit-common.md`'s un-owned-subsystems
  table calls *"the only path a non-developer reaches the engine through"* —
  the launcher/boot-request/settings-io/detect cluster, which has **no owner
  audit skill at all**. Dim 5 is one of the few generic sweeps that would reach
  it, and it doesn't.
- **Related**: #3456 (CLOSED — the identical recipe-blind-spot finding for
  Dim 9); #2974 (CLOSED — Dim 1's recipe proxy). `_audit-common.md`
  un-owned-subsystems table, "Launcher (boot/settings/detect)" row.
- **Suggested Fix**: Change the Dim 5 discovery command to
  `grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux tools | grep -v nifskope`
  and add a one-line note that `tools/nifskope` is vendored and deliberately
  excluded. Then re-word the "entire codebase" claim in the next report to name
  its actual scope.

---

---

### TD5-2026-09-05-02: the two Dim 5 grep patterns disagree with each other, and neither sees the `TBD` convention the codebase actually uses

- **Severity**: LOW
- **Dimension**: 5 (Stale Markers)
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md` — the Dimension 5
  **Discovery** block, both commands; live instance at
  `crates/plugin/src/esm/records/items.rs` (the `b"DNAM"` FNV `WEAP` arm, offset-20 read)
- **Status**: NEW
- **Age**: `items.rs` `TBD` — `67e1baafe`, 2026-06-09 (2.9 months). The pattern
  asymmetry predates it.
- **Effort**: trivial (≤30 min)
- **Description**: Two independent pattern defects, same root cause — the marker
  vocabulary is hand-written twice and never reconciled.

  **(a) The shader command is narrower than the source command.** Line 1 is
  `(TODO|FIXME|HACK|XXX)\b`; line 2 is `(TODO|HACK)` — it drops `FIXME` and
  `XXX` entirely, and drops the `\b` anchor. A `// FIXME` in `triangle.frag` or
  any of the 22 shaders / 15 GLSL includes would not be reported by the
  dimension that exists to report it. (I re-ran the shader scan
  case-insensitively across all four tokens: still 0, so nothing is hidden
  *today*.)

  **(b) The source command misses `TBD`, a convention in live use.** Exactly one
  site in the tree uses it, and the recipe has never seen it:

  > `// Offset 20 — next f32 present in the blob; semantic`
  > `// TBD (may or may not duplicate the NAM6 spread). Not`
  > `// stored; NAM6 remains the authoritative spread source.`

  On merit this site is **not** debt and I am not filing it as one — it is an
  honest documented-unknown that records its own resolution in place ("Not
  stored; NAM6 remains the authoritative spread source"), the same class as the
  documented staged-rollout exclusions. But it is precisely the shape this
  dimension hunts (an unresolved format semantic parked in a comment), and it
  sits four lines below a comment block whose neighbour was already the subject
  of a real finding — **#3324** (CLOSED) closed a false-premise comment in this
  very `DNAM` arm that *"sent two audits searching this blob."* A marker
  vocabulary that cannot see the one convention used in the most audit-prone
  comment block in the ESM parser is a measurable blind spot.
- **Evidence**:
  ```
  SKILL.md Dim 5 Discovery, command 1: grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux
  SKILL.md Dim 5 Discovery, command 2: grep -RInE '(TODO|HACK)' crates/renderer/shaders/
                                                    ^^^^^^^^^^ no FIXME, no XXX, no \b

  $ grep -RInE '\b(WIP|TBD|KLUDGE|XXX_)\b' crates byroredux --include='*.rs'
  crates/plugin/src/esm/records/items.rs:348:  // TBD (may or may not duplicate the NAM6 spread). Not
  # ^ one hit, never surfaced by four consecutive Dim 5 runs
  ```
- **Impact**: Two silent under-counts. (a) is the more dangerous half — the
  shader tree is where `feedback_shader_struct_sync.md` says lockstep drift
  hurts most, and a `FIXME` there is exactly the breadcrumb an auditor would
  want. Neither is hiding live debt today; both are measured, not assumed.
- **Related**: #3456 (CLOSED — Dim 9's regex under-count, 19 %, the direct
  precedent for this class of finding); #3324 (CLOSED — the false-premise
  comment four lines above the `TBD` site).
- **Suggested Fix**: Make both commands share one vocabulary —
  `(TODO|FIXME|HACK|XXX|TBD|WIP|KLUDGE)\b` — and point the second at
  `crates/renderer/shaders/` with the same pattern as the first. Add `TBD` to
  the exclusion guidance with the `items.rs` site as the worked example of a
  *legitimate* documented-unknown, so the next auditor doesn't over-file it.

---

## Complete Triage Table (all 23 sites)

Every hit from the two recipe commands, plus 3 sites surfaced only by the
broadened scans. `EXCLUDE` = documented false-positive class, no action.
Ages via `git blame`; none exceeds 6 months (threshold date: 2026-03-05).

### Recipe command 1 — `grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux` → 20 hits

| # | Marker | File | Commit | Date | Age | Verdict |
|---|---|---|---|---|---|---|
| 1 | `XXXX` | `crates/plugin/src/esm/reader.rs:856` | `71c707e7` | 2026-05-30 | 3.2 mo | EXCLUDE — ESM extended-size protocol tag |
| 2 | `XXXX` | `crates/plugin/src/esm/reader.rs:858` | `71c707e7` | 2026-05-30 | 3.2 mo | EXCLUDE — same block |
| 3 | `XXXX` | `crates/plugin/src/esm/reader.rs:862` | `71c707e7` | 2026-05-30 | 3.2 mo | EXCLUDE — `&sub_type == b"XXXX"`, live protocol compare |
| 4 | `XXXX` | `crates/plugin/src/esm/reader.rs:864` | `71c707e7` | 2026-05-30 | 3.2 mo | EXCLUDE — malformed-payload guard comment |
| 5 | `XXXX` | `crates/plugin/src/esm/reader.rs:872` | `71c707e7` | 2026-05-30 | 3.2 mo | EXCLUDE — payload-consume comment |
| 6 | `XXXX` | `crates/plugin/src/esm/reader.rs:1436` | `71c707e7` | 2026-05-30 | 3.2 mo | EXCLUDE — doc on the #1347 regression test |
| 7 | `XXXX` | `crates/plugin/src/esm/reader.rs:1438` | `71c707e7` | 2026-05-30 | 3.2 mo | EXCLUDE — same doc |
| 8 | `XXXX` | `crates/plugin/src/esm/reader.rs:1448` | `71c707e7` | 2026-05-30 | 3.2 mo | EXCLUDE — test fixture comment |
| 9 | `XXXX` | `crates/plugin/src/esm/reader.rs:1454` | `71c707e7` | 2026-05-30 | 3.2 mo | EXCLUDE — test fixture comment |
| 10 | `XXXX` | `crates/plugin/src/esm/reader.rs:1455` | `71c707e7` | 2026-05-30 | 3.2 mo | EXCLUDE — `extend_from_slice(b"XXXX")` fixture bytes |
| 11 | `XXXX` | `crates/plugin/src/esm/reader.rs:1484` | `71c707e7` | 2026-05-30 | 3.2 mo | EXCLUDE — assert message text |
| 12 | `FIXME` | `crates/bgsm/src/bgem.rs:137` | `edb0525e` | 2026-04-20 | **4.5 mo** (oldest) | EXCLUDE — *"Order matches the reference's `// FIXME` note"*; documents upstream, not our debt |
| 13 | `XXXX` | `crates/plugin/src/esm/records/misc/magic.rs:1354` | `01cfa3d0` | 2026-06-03 | 3.1 mo | EXCLUDE — `*b"XXXX"` wrong-type test sentinel |
| 14 | `XXXX` | `crates/plugin/src/esm/records/misc/magic.rs:1385` | `6873438c` | 2026-06-03 | 3.1 mo | EXCLUDE — same sentinel |
| 15 | `XXXX` | `crates/plugin/src/esm/records/misc/magic.rs:1428` | `6873438c` | 2026-06-03 | 3.1 mo | EXCLUDE — same sentinel |
| 16 | `XXXX` | `crates/plugin/src/esm/cell/wrld.rs:179` | `560c6741` | 2026-07-26 | 1.3 mo | EXCLUDE — *"through the XXXX extended-size escape"* |
| 17 | `FIXME` | `crates/plugin/src/esm/records/misc/world.rs:275` | `f8e59b4e` | 2026-08-13 | 0.7 mo | EXCLUDE — cites **OpenMW's** FIXME as evidence *against* its rule; our code is the correction |
| 18 | `FIXME` | `crates/nif/src/blocks/bs_geometry.rs:624` | `f8315a1b` | 2026-04-26 | 4.3 mo | EXCLUDE — cites **nifly's** FIXME at `BSGeometryMeshData::Sync:1709`; upstream reference |
| 19 | `TODO` | `byroredux/src/scene.rs:1481` | `e6192cc5` | 2026-05-14 | 3.7 mo | EXCLUDE — closure note, *"Closes the #242 consumer-side TODO (#1055)"*. **Premise re-verified**: #242 CLOSED, #1055 CLOSED, and `MeshRegistry.geometry_staging_pool` is live (`crates/renderer/src/mesh.rs`, declared + lazy-init + consumed). Accurate history, not a live marker |
| 20 | `TODO` | `byroredux/src/groundcover_translate.rs:252` | `e26d35f3` | 2026-08-12 | 0.8 mo | EXCLUDE — prose asserting a marker's **absence**: *"which is why the fallback lives in `GroundCoverPalette::resolve` rather than behind a `TODO` here"* |

### Recipe command 2 — `grep -RInE '(TODO|HACK)' crates/renderer/shaders/` → 0 hits

Re-run case-insensitively over all four tokens across 22 shader sources + 15
GLSL includes: still **0**. See TD5-2026-09-05-02(a) for the pattern asymmetry.

**MUST-NOT-DELETE verification**: `crates/renderer/shaders/triangle.frag` lines
~15–34 — attribution block **INTACT**. Contains the GLSL-PathTracer MIT notice
(`knightcrawler25/GLSL-PathTracer`, Copyright (c) 2019 Asif Ali, MIT), the
`src/shaders/common/{disney,sampling,pathtrace}.glsl` source list, the "MIT
requires this notice travel with the code" sentence, and the Burley 2012
SIGGRAPH Disney-BRDF citation with its explicit "no Disney code or assets are
used here" disclaimer. No edit has stripped or truncated it.

### Beyond the recipe — 3 sites the recipe cannot see

| # | Marker | File | Commit | Date | Verdict |
|---|---|---|---|---|---|
| 21 | `hack` (lowercase) | `crates/physics/src/convert.rs:1139` | `4de5e78e` | 2026-08-14 | EXCLUDE — English prose: *"a true uniform scale and not a translation hack"*, in a test doc |
| 22 | `hack` (lowercase) | `crates/plugin/src/esm/records/misc/world.rs:1475` | `08d4783e` | 2026-08-31 | EXCLUDE — gameplay noun: *"scripts on successful hack"* (terminal hacking, `TERM` SCRI) |
| 23 | `to-do` (hyphenated) | `crates/ui/src/avm2_host.rs:1475` | `422c68f7` | 2026-08-14 | EXCLUDE — English prose: *"it is a to-do list for the catalog's `kind`/response metadata"* |
| 24 | `TBD` | `crates/plugin/src/esm/records/items.rs:348` | `67e1baaf` | 2026-06-09 | EXCLUDE on merit — documented unknown that records its own resolution; **drives TD5-2026-09-05-02(b)** as recipe evidence |

## Diff vs prior audits

| Audit | Recipe hits | Live markers | Composition |
|---|---|---|---|
| 2026-08-16 | 20 | 0 | baseline |
| 2026-08-27 | 20 | 0 | unchanged |
| 2026-08-30 | 20 | 0 | unchanged |
| **2026-09-05** | **20** | **0** | **unchanged** (same 20 files:lines, same commits) |

Nothing was added, deleted, or re-blamed in this window. The `.claude/issues/`
convention of filing an issue instead of leaving a bare marker continues to
hold at 100 %.

## Severity Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| **LOW** | **2** |

Both LOW findings target the discovery recipe itself, not code. Neither is
hiding live debt today — both were verified empty before filing.

---

### TD6-2026-09-05-02: `crates/core/src/stealth.rs` justifies its zero-consumer state with a blocker that shipped — #446 is closed and M42 delivered seven procedure runtimes


- **Severity**: LOW
- **Dimension**: 6 — Stub & Placeholder Implementations
- **Status**: NEW
- **Effort**: trivial (≤30 min)

**Location**: `crates/core/src/stealth.rs` — module docstring, "## Status: greenfield,
math-only, by design" section (`:20-33`)

**Description**

`stealth.rs` is a zero-production-consumer module (`detection_score` / `classify`
have no callers outside their own tests — verified). That is fine *by this
dimension's own rule* — a consumer-less module is expected-and-fine **when documented
as such** — and this one is documented unusually well.

The problem is that the documentation's load-bearing justification is now false. The
docstring reads:

> *"Nothing in the engine feeds this yet: there's no AI-package evaluator, no
> line-of-sight/vision system, no alert-state component, no sneak/crouch flag (see
> the survey behind this module — ROADMAP.md's M42 "AI packages" milestone, which
> this formula will eventually plug into, is Tier 7 and **blocked on `PACK` record
> parsing, #446**)."*

and closes: *"the ECS wiring … **waits until M42 gives it something to drive**."*

Three checkable claims, all stale:

1. **"#446 … blocked on `PACK` record parsing"** — #446 is **CLOSED**
   (`FO3-3-04: PACK AI package records skipped`), closed by `90e6b068` per
   `ROADMAP.md:794`. `crates/plugin/src/esm/records/misc/pack.rs` is 1,895 LOC of
   shipped PACK parsing.
2. **"there's no AI-package evaluator"** — `package_conditions_pass`
   (`byroredux/src/npc_spawn/ai_package.rs:31`) is the M42.2 CTDA package evaluator,
   and `ambient_ai_package_system` (`:572`) is registered unconditionally as a
   `Stage::Update` exclusive at `byroredux/src/boot.rs:1053`.
3. **"waits until M42 gives it something to drive"** — M42 has shipped seven
   procedure runtimes (Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol, M42.1–M42.9;
   `ROADMAP.md:794`).

The *conclusion* (no consumer) remains correct — line-of-sight, alert state and a
sneak/crouch flag genuinely still do not exist. But the stated **precondition has been
met**, so this stub is no longer *blocked*; it is *unscheduled*, and nothing records
that transition. A stub whose deferral condition has silently expired is the exact
case where "documented, therefore fine" stops holding.

**Evidence**

```
$ gh issue view 446 --json state          → {"state":"CLOSED"}
$ grep -n "fn package_conditions_pass\|fn ambient_ai_package_system" byroredux/src/npc_spawn/ai_package.rs
31:fn package_conditions_pass(
572:pub(crate) fn ambient_ai_package_system(world: &World, _dt: f32) {
$ grep -n "ambient_ai_package_system" byroredux/src/boot.rs
1053:    scheduler.add_exclusive(Stage::Update, crate::npc_spawn::ambient_ai_package_system);
```

The module already carries one dated correction of this same class (#2979, *"One
correction to 'nothing feeds this yet'…"*), so the pattern is known here — this is the
next instance of it, not a first offence.

**Impact**

Low but widening. The docstring names `HitEvent.sneak_attack` as "this module's
concrete future hook point"; that field is hardcoded `false` at
`byroredux/src/combat.rs:274` and `:638`. Since the SDK/extension surface landed
(`21a840d5`, 2026-08-25) that constant is now re-exported to sandboxed guest code
through `byroredux/src/extensions.rs:2967,4741` — i.e. it is observable by any mod
loaded via the shipped `--extension` flag, which will see `sneak_attack == false` for
every hit forever. The stub itself is still unreached, which is why this stays LOW.

The concrete harm is misdirection: a contributor reading this module to decide what
to build next is told to go wait on `#446`/M42, both of which are done.

**Related**

- #2979 (CLOSED) — the prior correction to this same "nothing feeds this yet" claim,
  in the sibling `crates/core/src/combat.rs`.
- #3482, #2962 (CLOSED) — prior `stealth.rs` audits; neither touched the M42/#446
  gating claim.
- Overlaps Dim 3 (Stale Documentation). Filed here because Dim 3's discovery recipe
  — path gate, symbol advisory, GPU-size cross-checks — cannot surface an expired
  *deferral condition*, and because the claim's only function is to justify a stub.
  If the merge phase prefers, fold it into Dim 3 rather than reporting twice.

**Suggested Fix**

Replace the `#446`/"Tier 7"/"blocked on PACK record parsing" clause with what is
actually missing today — no line-of-sight/vision system, no `AlertState` component,
no sneak/crouch input — and drop "waits until M42 gives it something to drive"
(M42 has). One sentence; the rest of the section is accurate and should be kept.

---

## Negative results (checked, deliberately not filed)

These are the areas the dimension brief named. Each was checked and is clean; they
are recorded so the next audit can diff rather than re-derive.

| Checked | Result |
|---|---|
| `unimplemented!` / `todo!()` / `panic!("not …")` | **0 hits.** Recipe re-run at HEAD. |
| Empty-bodied `fn … {}` outside `#[cfg(test)]` | **2**, both legitimate: `DynStorage::shrink_sparse_tail` (`crates/core/src/ecs/storage.rs:113` — documented trait default, *"Default is a no-op — `PackedStorage` … has nothing to release"*) and a `log::Log::flush` impl in a test-corpus binary. **No trait impl contradicts its trait docs.** |
| Functions whose whole body is `Ok(())` / `None` / `Vec::new()` / `Default::default()` | 18 hits, **all** trait-default or `Self::default()` constructors. No stub. |
| Console commands in `byroredux/src/commands/` | 76 registered commands; **none** print `TODO` or no-op. Every one is backed by a resource that is inserted in production. The closest prior finding of this class, #2976 (`InputAction::Block` bound but with no gameplay effect), is **fixed and not regressed** — `byroredux/src/combat.rs:123` now reads `actions.is_held(InputAction::Block)`. |
| Per-game ESM record coverage vs. ROADMAP compat matrix | **No matrix row claims support the records tree stubs.** `crates/plugin/src/esm/records/dispatch_misc_stub.rs` bulk-dispatches 30 long-tail record types through `parse_minimal_esm_record` (EDID + optional FULL). All 30 index fields have **zero production consumers** (only a count assertion in `crates/plugin/tests/parse_real_esm.rs`) — but this is a deliberate, documented tier with a written graduation protocol (*"When a real consumer arrives … replace the dispatch arm + `MinimalEsmRecord` map with a dedicated parser pair via the established #808/#809 pattern"*), and two types have already graduated (`SOUN`→`parse_soun` per #2372, `IMAD`→`parse_imad`). ROADMAP's compat matrix is a NIF-parse-rate/cell matrix and makes no ESM-record claim; the four docs that *do* describe these records — `docs/COMPATIBILITY.md:99` (Grass `[ ]`, *"Record type dispatched; no instanced grass renderer"*), `docs/engine/exal-groundcover.md:285` (*"parsed only as a `MinimalEsmRecord` stub … with no consumer"*), `docs/engine/esm-records.md:446`, `docs/feature-matrix.md` — are all accurate. Nothing to file. |
| Young crates: `crates/mod-runtime` | **Consumer status has changed** and is worth flagging to the merge phase: `_audit-common.md`'s un-owned-subsystems table still says *"Still has **no consumer in the engine**"*, but `byroredux/src/extensions.rs:22` imports it, `boot.rs:739-1849` registers ~10 `extension_*` systems, and `--extension` / `--extension-grant` are shipped, README-documented flags. Its binding surface (`runtime.rs`, 4,588 LOC) contains **no stubs** — every `deliver_*` returns a real deferred command batch. The doc-rot half belongs to Dim 3/4. |
| Young crates: `crates/sdk`, `crates/pex`, `crates/save`, `crates/hkx`, `crates/scripting` | No undocumented stubs. `sdk`'s `not implemented` strings are `ManifestError`/`CompatibilityError` variants (real error paths, not stubs). `sdk.compat` — the one user-reachable SDK console command — is fully backed by `CompatibilityRegistry` and honestly reports `unsupported` dispositions. `crates/pex/src/model.rs:255` documents an unread field as *"**Currently unread** — no consumer anywhere in the workspace"*: the healthy pattern. |
| NIF Havok constraint stub tier | **Exemplary, and shrinking.** `is_havok_constraint_stub` (`crates/nif/src/lib.rs:189`) is down to 4 types (`bhkBallAndSocketConstraint`, `bhkStiffSpringConstraint`, `bhkGenericConstraint`, `bhkBallSocketConstraintChain`); five graduated under #3713/#3330/#3792. Under-reads route to a dedicated `stubbed_drift_histogram` so they cannot contaminate real drift telemetry. Not debt — the reference implementation of how to carry a stub. |
| Documented zero-consumer modules (`crates/facegen` #3544, `esm/cell::DecalData` #3638, `crates/physics/src/water.rs` WATAL Phase 3, `character/affliction.rs`) | All four state their zero-consumer status explicitly, name the blocker, and (for affliction) cite `feedback_no_guessing` for why the data is PENDING. Correct handling; not filed. `crates/core/src/stealth.rs` is the one member of this family whose blocker has expired — TD6-2026-09-05-02. |
| `resolve_palette_for_chain(chain, authored)` — always called with `Vec::new()` | Not a stub. `byroredux/src/groundcover_translate.rs:247-252` pre-empts the finding: *"today it is empty for every game and the built-in default carries the feature. That is the intended end state for content with no vegetation data, not a stub — which is why the fallback lives in `GroundCoverPalette::resolve` rather than behind a `TODO` here."* Verified: the sole production caller is `scene/world_setup.rs:622`. |
| `crates/game-detect::detect` — Steam-only | Documented scope limit (*"the Windows registry and GOG probes described in the plan are not implemented"*), and the function does real work. Not a stub. |

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 1 |
| **Total** | **2** |

The panic-stub class is empty and the comment-tagged stub class is, without
exception, correctly documented — this codebase carries its stubs unusually well.
Both findings are about *stub bookkeeping* rather than stub code: one fix that was
written, reviewed and issue-closing but never merged to `main`, and one deferral
condition that has been satisfied without anyone updating the stub that waits on it.

---

### TD7-2026-09-05-01: Five of the six `GpuRayBudget` policy ceilings are hand-retyped as shader loop bounds — only `glass_ray_limit` was ever derived


- **Severity**: LOW
- **Dimension**: 7 — Magic Numbers & Hardcoded Constants
- **Location**: `crates/renderer/src/vulkan/scene_buffer/ray_budget.rs::AdaptiveRayBudget::settings_for_tier` · `crates/renderer/shaders/triangle.frag` (the `MAX_SHADOW_RAYS`, `MAX_PATH_SEGMENTS`, `MAX_SHADED_HITS`, `MAX_REFRACT_PASSTHRUS` declarations) · `crates/renderer/shaders/volumetrics_inject.comp` (the `MAX_FROXEL_LIGHTS` declaration)
- **Status**: NEW
- **Effort**: small (≤2 h)
- **Description**: `GpuRayBudget` is a seven-field CPU→GPU quality contract. `settings_for_tier` picks the per-tier values on the Rust side; each consuming shader then needs a *compile-time* ceiling for its bounded loop, which must be ≥ the tier-3 value. Today that ceiling is an independently hand-typed GLSL literal for every field except `glass_ray_limit`, which #2686 / SAFE-D7-01 fixed by deriving all four tiers from `GLASS_RAY_BUDGET`. The sibling fields never got the same treatment, so the tier table and its shader-side ceilings are five pairs of literals that only happen to match.

  Worse, the runtime clamp *repeats* the ceiling a second time in the same expression (`clamp(rayBudget.directShadowSamples, 1u, 8u)` sits one line below `const int MAX_SHADOW_RAYS = 8;`), so each field is written out two or three times.

- **Evidence**: current tier-3 (`_ =>`) arm vs the shader literals that must track it:

  | `GpuRayBudget` field | tier-3 value (`ray_budget.rs`) | shader-side ceiling | second copy (runtime clamp) |
  |---|---|---|---|
  | `glass_ray_limit` | `GLASS_RAY_BUDGET` | *derived* — #2686 | n/a |
  | `direct_shadow_samples` | `8` | `triangle.frag`: `const int MAX_SHADOW_RAYS = 8;` | `clamp(rayBudget.directShadowSamples, 1u, 8u)` |
  | `max_path_segments` | `6` | `triangle.frag`: `const int MAX_PATH_SEGMENTS = 6;` | `clamp(rayBudget.maxPathSegments, 1u, 6u)` |
  | `max_shaded_hits` | `2` | `triangle.frag`: `const int MAX_SHADED_HITS = 2;` | `clamp(rayBudget.maxShadedHits, 1u, 2u)` |
  | `volumetric_light_cap` | `8` | `volumetrics_inject.comp`: `const uint MAX_FROXEL_LIGHTS = 8u;` | `clamp(…, 1.0, float(MAX_FROXEL_LIGHTS))` |
  | `quality_tier` (max) | `3` (`observe`'s `self.tier < 3`, the `_ =>` arm) | `triangle.frag`: `min(rayBudget.qualityTier, 3u)`, and `const int MAX_REFRACT_PASSTHRUS = 8;` — which is `2 + 3 * 2`, i.e. the tier count baked into a *second* independent literal |

  `volumetric_light_cap` reaches the shader as a float through `fog_reference[2]` in `context/post_passes.rs`, which reads `current_ray_budget(...).volumetric_light_cap as f32`. No file is `#include`ing a shared definition for any of these.

  The existing guards do not close the loop. `shader_contract_tests.rs::bounded_path_preserves_the_accepted_segment_and_diffuse_budgets` and `gi_zero_budget_is_a_true_no_ray_floor` assert the *shader source text* (`frag.contains("const int MAX_PATH_SEGMENTS = 6;")`, `frag.contains("clamp(rayBudget.maxPathSegments, 1u, 6u)")`) — neither ever names `AdaptiveRayBudget::settings_for_tier`. `MAX_SHADOW_RAYS` and `MAX_FROXEL_LIGHTS` have no test at all.

- **Impact**: raising a tier-3 value in `ray_budget.rs` is a **silent no-op**: the shader clamps the uploaded value back down to its own hardcoded ceiling and the whole suite stays green (the string-matching tests only fire if the *shader* changes). The reverse edit is equally invisible. No memory-safety consequence — every loop is `for (…; i < CEILING; …) { if (i >= runtime_limit) break; }` with no array indexed by the counter, so this is **not** the HIGH over/underflow trigger. Blast radius is a tuning pass that appears to do nothing on a GPU-adaptive quality ladder, which is exactly the failure #2686 was filed for.
- **Related**: #2686 / SAFE-D7-01 (the one field that was fixed) · #2265 / TD7-001 (same defect class, three GLSL files) · #2045 / TD7-101 · #3745 / TD7-2026-08-30-01 · TD7-2026-09-05-02 below (the gate that should have caught this cannot see function-local `const`s)
- **Suggested Fix**: add `MAX_DIRECT_SHADOW_SAMPLES`, `MAX_PATH_SEGMENTS`, `MAX_SHADED_HITS`, `MAX_FROXEL_LIGHTS` and `MAX_RAY_QUALITY_TIER` to `crates/renderer/src/shader_constants_data.rs`; have `settings_for_tier`'s `_ =>` arm use them (exactly as it already uses `GLASS_RAY_BUDGET`) and have `triangle.frag` / `volumetrics_inject.comp` take them from the `#include`d header instead of declaring locals. Then replace the string-matching assertions with the relationship that actually matters — `settings_for_tier(3).max_path_segments == MAX_PATH_SEGMENTS`, etc.

---

---

### TD7-2026-09-05-02: #3815's shader-constant provenance gate is blind to function-local `const`s, every `#define`, and the whole `shaders/include/` tree


- **Severity**: LOW
- **Dimension**: 7 — Magic Numbers & Hardcoded Constants
- **Location**: `crates/renderer/src/shader_constants.rs::top_level_shader_constants`, `::renderer_shader_sources`, `::every_top_level_shader_constant_has_one_provenance`
- **Status**: NEW (Regression-adjacent to #3815, CLOSED 2026-09-03 — the gate landed, its coverage claim is broader than its implementation)
- **Effort**: medium (≤1 day)
- **Description**: #3815 replaced a per-name allowlist with a structural check whose doc comment says it will *"scan the complete shader directory rather than maintaining a growing per-name allowlist."* Three independent narrowings mean the check reaches only a minority of shader constant declarations, and every constant that currently bypasses `shader_constants_data.rs` sits in one of the blind spots.
- **Evidence**: read against current code, three separate filters:
  1. **`renderer_shader_sources()` is not recursive and excludes `.glsl`.** It calls `std::fs::read_dir` on `shaders/` once and keeps only `Some("frag") | Some("vert") | Some("comp")`. `shaders/include/` is never opened and `.glsl` is not in the extension list, so **no include file is scanned at all** — including `include/shader_constants.glsl` itself.
  2. **`top_level_shader_constants` requires brace depth 0** on all four tokens of the `const TYPE NAME =` window (`*qualifier_depth == 0 && *ty_depth == 0 && *name_depth == 0 && *equals_depth == 0`). Every function-local `const` is invisible by construction — the doc comment states this as intent, but function-local is where this codebase actually puts its ray/loop budgets.
  3. **Only `const` with type `float | uint | int` is matched** (`matches!(ty.as_str(), "float" | "uint" | "int")`). `#define`, `vec*`, `ivec*`, `bool` and array constants are all unreachable — and `#define` is the form the generated header itself emits, so the "shared-name redeclaration" branch can only ever fire against the `const` spelling.

  What is currently un-gated as a result (all verified present today):
  - `include/pbr.glsl`: `#define SPECULAR_AA_VARIANCE 0.25`, `#define SPECULAR_AA_THRESHOLD 0.2` — both filters 1 and 3.
  - `include/lighting.glsl`: `const int REFLECTION_LIGHT_CANDIDATES = 4;`, `const uint GI_VISIBLE_LIGHT_CAP = 2u;` — the latter is the "first two VISIBLE contributors" that `shader_constants_data.rs`'s `GI_HIT_LIGHT_CAP` doc comment describes in prose, with no mechanical link.
  - `include/shadow_transport.glsl`: `const int MAX_GLASS_INTERFACES = 4;`
  - `include/mesh_id.glsl`: `const uint MESH_ID_NO_HISTORY_BIT = 0x80000000u;` (see TD7-2026-09-05-03).
  - `triangle.frag`: `MAX_SHADOW_RAYS`, `MAX_PATH_SEGMENTS`, `MAX_DIFFUSE_BOUNCES`, `MAX_SHADED_HITS`, `MAX_REFRACT_PASSTHRUS`, `RT_LOD_SCALE`, `RT_LOD_REFLECT`, `RT_LOD_GI`, `AMBIENT_FILL`, `RESERVOIR_W_CLAMP`, `RESTIR_M_CAP`, `SPATIAL_SAMPLES`, `SPATIAL_RADIUS`, `SPATIAL_M_CAP`, `TEMPORAL_NORMAL_COS` — all function-local, all filter 2.

  The gate is not vacuous: `shader_constant_provenance_gate_rejects_synthetic_shared_redeclaration` proves the shared-name branch works, and `SHADER_LOCAL_CONSTANT_EXEMPTIONS` holds 17 entries for the top-level declarations it *does* see (in `ssao.comp`, `svgf_atrous.comp`, `volumetrics_inject.comp`). But the exemption list being 17 long against a reachable population of ~17 means the gate is currently catching zero live violations while ~20 real bypasses sit just outside its reach.

- **Impact**: the codebase believes it has a structural single-source-of-truth guarantee for shader constants (`feedback_shader_struct_sync.md` treats this as the lockstep mechanism) when in practice the guarantee covers only top-level scalar `const`s in `.frag`/`.vert`/`.comp` files. Every finding in this dimension's shader half — TD7-2026-09-05-01 and -03 — is a constant the gate could not see. Low severity because no drift has actually shipped; the debt is a false sense of coverage that will let the next one through.
- **Related**: #3815 (the gate) · #1780 / D14-LOW-01 (`caustic_splat.comp` + `water.frag` missing from an earlier lockstep test — same "the check does not reach the file" shape) · TD7-2026-09-05-01 · TD7-2026-09-05-03
- **Suggested Fix**: walk `shaders/` recursively and add `glsl` to the extension filter; extend the lexer to recognise `#define NAME <literal>` alongside `const`; drop the `brace_depth == 0` requirement (or report function-local declarations under a separate, softer violation class so the ~15 legitimately stage-local ones in `triangle.frag` can be exempted deliberately rather than by accident). Expect the exemption list to grow — that is the point: each entry becomes a recorded decision instead of an invisible bypass.

---

---

### TD7-2026-09-05-03: The mesh-ID no-history bit and its complement mask are hand-typed at five GLSL sites — including the shader that *writes* the bit — and have no Rust-side constant at all


- **Severity**: LOW
- **Dimension**: 7 — Magic Numbers & Hardcoded Constants
- **Location**: `crates/renderer/shaders/include/mesh_id.glsl` (`MESH_ID_NO_HISTORY_BIT`) · `crates/renderer/shaders/triangle.frag` (the `outMeshID` write and the two `sortedInstanceId` / `stableSurfaceId` masks) · `crates/renderer/shaders/caustic_splat.comp` (the `meshIdRaw` test and mask)
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Description**: bit 31 of the `R32_UINT` mesh-ID G-buffer attachment is the `ALPHA_BLEND_NO_HISTORY` flag: set, it switches the low 31 bits into the alpha draw-index namespace and tells TAA/SVGF the pixel has no stable temporal history. It has exactly one named declaration — `const uint MESH_ID_NO_HISTORY_BIT = 0x80000000u;` in `include/mesh_id.glsl` — and that header is `#include`d by only three of its five consumers (`taa.comp`, `svgf_atrous.comp`, `svgf_temporal.comp`). The two that do not include it are the **producer** and the caustic reader, both of which hand-type the raw literal.
- **Evidence**:
  - `include/mesh_id.glsl` declares the bit and the `meshIdHasStableHistory` / `stableMeshIdsMatch` helpers. `grep -rn 'mesh_id.glsl'` over the shader tree returns exactly three `#include` lines: `taa.comp:6`, `svgf_atrous.comp:6`, `svgf_temporal.comp:6`.
  - `triangle.frag` — the writer — emits `outMeshID = meshIdBase | (alphaBlendFrag ? 0x80000000u : 0u);` and masks the two ID lanes with `& 0x7FFFFFFFu` twice, immediately above it. No include.
  - `caustic_splat.comp` reads `if ((meshIdRaw & 0x80000000u) == 0u) return;` then `uint meshId = meshIdRaw & 0x7FFFFFFFu;`. No include.
  - **Nothing on the Rust side declares it.** `scene_buffer/constants.rs`, `vulkan/context/helpers.rs` and `vulkan/pipeline.rs` each describe `0x80000000` in a *comment* — three separate prose restatements of a bit with no code-level definition on their side of the boundary. (`shader_constants_data.rs`'s `NORMAL_ALPHA_SPEC_BIT` / `PARALLAX_ALPHA_HEIGHT_BIT` / `DBG_VIZ_SELECTED_LIGHT` all share the numeric value `0x80000000` for unrelated fields, so a value-based search cannot disambiguate this one either.)
  - The complement `0x7FFFFFFFu` is written three times and never expressed as `~MESH_ID_NO_HISTORY_BIT`.
- **Impact**: the producer and one reader can drift from the header independently, and a search for the bit's definition from the Rust side finds only comments. This is the identical shape as #2265 (one 8-layer budget, three independent GLSL declarations), #2045 (`INST_RENDER_LAYER_SHIFT`/`_MASK` hand-written in `triangle.frag`) and #3745 (RT reach budgets hand-typed at six sites) — all closed; this instance was missed because the declaration lives in `include/`, which the provenance gate does not scan (TD7-2026-09-05-02). Low severity: the bit is correctly typed at every site today, so nothing is currently wrong.
- **Related**: #2265 / TD7-001 · #2045 / TD7-101 · #3745 / TD7-2026-08-30-01 · #1780 / D14-LOW-01 · TD7-2026-09-05-02
- **Suggested Fix**: move the bit into `crates/renderer/src/shader_constants_data.rs` (it is a genuine cross-CPU/GPU attachment ABI, and the Rust-side comments in `scene_buffer/constants.rs` and `context/helpers.rs` already want to reference it by name), emit a companion `MESH_ID_STABLE_MASK` `#define`, and have `include/mesh_id.glsl` consume the generated header rather than redeclaring. Then `#include` it from `triangle.frag` and `caustic_splat.comp` and replace all five literals.

---

---

### TD7-2026-09-05-04: `shader_constants_data.rs` hand-copies `MAX_BONES_PER_MESH = 144` where its own re-export pattern and an existing build-dependency allow a derivation


- **Severity**: LOW
- **Dimension**: 7 — Magic Numbers & Hardcoded Constants
- **Location**: `crates/renderer/src/shader_constants_data.rs::MAX_BONES_PER_MESH` (source of truth: `crates/core/src/ecs/components/skinned_mesh.rs::MAX_BONES_PER_MESH`)
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Description**: `shader_constants_data.rs` exists precisely so a shared constant has one definition, and it already resolves ~40 of its entries by re-export — `WORLD_UNITS_PER_METER`, `LEGACY_LIGHT_CULL_RANGE_MULTIPLIER`, every `COMBUSTION_*` / `FLAME_*` / `EXPLOSION_*`, every `VISIBILITY_LAYER_*`, `WATER_*`, `DEFAULT_GLASS_BLUR_SCALE`, `DEFAULT_WATER_WAVE_AMPLITUDE` — all written as `byroredux_core::…`. `MAX_BONES_PER_MESH` is the outlier: it restates the literal `144` and points at core only in a comment (*"see `byroredux_core::ecs::components::skinned_mesh::MAX_BONES_PER_MESH` for the vanilla-content survey that fixes this ceiling at 144"*).
- **Evidence**:
  - `crates/core/src/ecs/components/skinned_mesh.rs` declares `pub const MAX_BONES_PER_MESH: usize = 144;`, publicly re-exported by `crates/core/src/ecs/components/mod.rs` (`pub use skinned_mesh::{SkinnedMesh, MAX_BONES_PER_MESH};`) — the exact path the doc comment names.
  - `crates/renderer/Cargo.toml` lists `byroredux-core` under `[build-dependencies]`, so `build.rs`'s `include!("src/shader_constants_data.rs")` resolves `byroredux_core::…` paths — proven by the ~40 sibling entries that already do.
  - The only guard is `shader_constants.rs::max_bones_per_mesh_matches_core`, a runtime `assert_eq!` in a `#[cfg(test)]` module. A re-export would make the same fact a compile-time identity.
  - **Deliberately not flagged as a sibling**: `VERTEX_STRIDE_FLOATS = 26` looks like the same defect but is *forced* — its source of truth is `crate::vertex::Vertex`, and `build.rs` cannot import the crate it builds. Its `size_of::<Vertex>()` test is the correct mitigation there, not a workaround.
- **Impact**: minimal in practice (the test catches divergence), but it is one hand-maintained copy of a survey-derived ceiling in the one file whose entire purpose is to not have those. Editing core's value and running only `-p byroredux-core` leaves the shader header stale until the renderer's test suite runs.
- **Related**: #1758 / TD7-001 (`SKIN_WORKGROUP_SIZE` — the sibling skinning constant, fixed the same way) · #1451 / SKIN-02 · TD7-2026-09-05-02
- **Suggested Fix**: `pub const MAX_BONES_PER_MESH: u32 = byroredux_core::ecs::components::MAX_BONES_PER_MESH as u32;`, matching the surrounding re-export style. Keep `max_bones_per_mesh_matches_core` — it becomes trivially true, which is the desired end state.

---

---

### TD7-2026-09-05-05: `parse_weather_data` decodes the WTHR DATA payload with bare offsets while the named `SKYRIM_DATA_SIZE` sits six lines above it


- **Severity**: LOW
- **Dimension**: 7 — Magic Numbers & Hardcoded Constants
- **Location**: `crates/plugin/src/esm/records/weather.rs::parse_weather_data` (constant: `SKYRIM_DATA_SIZE`, declared immediately above it)
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Description**: `SKYRIM_DATA_SIZE: usize = 19` is declared directly above `parse_weather_data` and is used by the record dispatcher (`b"DATA" if sub.data.len() >= SKYRIM_DATA_SIZE`) and by a test fixture — but not by the decoder it documents. The decoder instead runs a ten-step ladder of bare length guards (`data.len() > 3`, `> 4`, `> 5`, `> 7`, `> 9`, `> 10`, `> 11`, `>= 15`, `> 16`, `> 18`) paired with bare indices (`data[3]` … `data[18]`), where the final `> 18` is the same gate as `>= SKYRIM_DATA_SIZE`.
- **Evidence**: the last guard, `if data.len() > 18 { record.wind_direction = data[17]; record.wind_direction_range = data[18]; }`, is exactly `len() >= SKYRIM_DATA_SIZE` spelled as a literal. The ladder also mixes `>` and `>=` conventions for the same predicate shape (`>= 15` for `data[14]`, `> 16` for `data[16]`) — every guard is arithmetically **correct**, verified index by index; the inconsistency is a readability cost, not a bug, and is why this stays LOW rather than being disproved outright.
- **Impact**: the byte layout of a shared FO3/FNV/Skyrim sub-record lives as twenty scattered integers, so a layout correction has to be applied in ten independent places with no compiler assistance and no shared name to grep for. The record has already had one layout correction (the function's own doc comment records *"byte 10 is thunder/lightning frequency and byte 11 is the classification bitmask (not byte 14)"*), which is evidence this layout does get revised.
- **Related**: #1631 / TD7-002 (CNTO sub-record size duplicated across two record parsers — same class, closed) · #2597 / FO4-D4-01 (bare `(130..=139)` band instead of named constants)
- **Suggested Fix**: replace the trailing `data.len() > 18` with `data.len() >= SKYRIM_DATA_SIZE`, and give each field a `const WTHR_<FIELD>_OFFSET: usize` (or a small `(offset, len)` table the ladder iterates) so the layout is stated once. **Explicitly not proposed** for the WATR `DNAM` decoder in `records/misc/water.rs`, which uses the same raw-offset style but annotates every offset against its xEdit definition inline — the named-constant fix there would add indirection without adding information.

---

## Summary

| Severity | Count |
|---|---|
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 5 |

**No HIGH promotion claimed.** The dimension's single HIGH trigger is a magic number that would silently over/underflow under documented use. The closest candidate is TD7-2026-09-05-01, and it does not qualify: every affected shader loop is `for (i = 0; i < CEILING; ++i) { if (i >= runtime_limit) break; }` with no array indexed by the counter, and the runtime value is `clamp`-ed into range before use — a CPU-side increase past the ceiling is silently *clamped down*, never an out-of-range index or a wrapped counter. Filed LOW with the arithmetic stated rather than promoted on the strength of the phrase "silently".

**Effort**: 3 trivial, 1 small, 1 medium.

**Most significant**: TD7-2026-09-05-02 — the provenance gate that closed two days ago (#3815) reaches only top-level scalar `const`s in `.frag`/`.vert`/`.comp` files, so `shaders/include/` and every function-local constant are outside it; both other shader findings in this dimension live in exactly those blind spots.

---

### TD8-2026-09-05-01: The whole FormId→Entity single-root index subsystem is dead, and both milestones its `#[allow(dead_code)]`s name as gates closed on 2026-08-31


- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/cell_loader/persistent_ref_index.rs` (`resolve_persistent_ref`, `invalidate`), `byroredux/src/cell_loader/cell_root_ref_index.rs` (`resolve_cell_root_ref`, `invalidate`), `byroredux/src/cell_loader/form_id_root_index.rs` (`resolve`), `byroredux/src/components.rs` (`PersistentRefIndex`, `CellRootRefIndex` — struct + 4 field-level allows), `byroredux/src/boot.rs` (the two `insert_resource` calls)
- **Status**: NEW (successor condition to #3455, CLOSED 2026-08-28)
- **Effort**: small (≤2 h)
- **Age**: `PersistentRefIndex` landed with EX-09/#2370; `CellRootRefIndex` + `form_id_root_index` split out later as its sibling. #3455 last re-justified them on 2026-08-28.

**Description**
Three modules (501 lines total, ~186 of them production), two ECS `Resource`s, two live `boot.rs` insertions and **8 `#[allow(dead_code)]` attributes** implement an `O(1)` FormId→Entity lookup scoped to a single `CellRoot`. Nothing in production has ever called any of it.

This is the second time the "landed ahead of its consumer" justification has been checked and found expired. #3455 (2026-08-27) established the rule for this exact code: EX-14/15 (#2369) had closed without wiring the index, so the comments were rewritten to name **EX-16 (#2372) as the one live gate**, and `persistent_ref_index.rs`'s module doc wrote down its own deletion condition verbatim:

> `//! #3455 — EX-14/15 (#2369) was the other named consumer and closed`
> `//! 2026-08-26 without wiring the index, so **EX-16 (#2372) is the only live`
> `//! gate**. […] If EX-16 reaches persistent refs by another route, delete this`
> `//! module, the PersistentRefIndex resource and the boot.rs insertion together —`
> `//! form_id_root_index::resolve stays live via CellRootRefIndex.`

**EX-16 (#2372) closed 2026-08-31T14:46:47Z**, and it too shipped without wiring the index. The stated deletion condition is now satisfied on the module's own terms.

Worse, the escape hatch in that same sentence is **false**: `form_id_root_index::resolve` does *not* stay live via `CellRootRefIndex`, because `CellRootRefIndex` is equally dead. A future auditor who trusts that line would delete half the subsystem and leave the other half — the exact failure mode `_audit-common.md`'s "a fact that rots becomes a false premise" note warns about.

**Evidence**
```
$ gh issue view 2369 --json state,closedAt   → CLOSED 2026-08-31T16:01:41Z   (EX-14/15)
$ gh issue view 2372 --json state,closedAt   → CLOSED 2026-08-31T14:46:47Z   (EX-16)

$ grep -RIn "resolve_persistent_ref\|invalidate\|resolve_cell_root_ref" --include="*.rs" crates byroredux tools
  → definitions + same-module tests only; zero production call sites

$ grep -RIn "form_id_root_index::resolve" --include="*.rs" byroredux
  byroredux/src/components.rs:1522            # doc comment
  byroredux/src/cell_loader/persistent_ref_index.rs:27   # doc comment (the false claim)
  byroredux/src/cell_loader/persistent_ref_index.rs:59   # inside the dead wrapper
  byroredux/src/cell_loader/cell_root_ref_index.rs:32    # inside the dead wrapper

$ grep -RIn "PersistentRefIndex\|CellRootRefIndex" --include="*.rs" byroredux | grep -v tests | grep -v components.rs
  byroredux/src/boot.rs:524:    world.insert_resource(crate::components::PersistentRefIndex::new());
  byroredux/src/boot.rs:525:    world.insert_resource(crate::components::CellRootRefIndex::new());
  # + the two `use` lines inside the dead modules themselves
```
`wc -l`: `persistent_ref_index.rs` 217, `cell_root_ref_index.rs` 180, `form_id_root_index.rs` 104.

**Impact**
Two `Resource`s occupy slots in every live `World` and are enumerated by the save-registry completeness list (`byroredux/src/save_io/registry_completeness_tests.rs`, `CellRootRefIndex` row) without ever holding data. Three modules with full test suites must be kept compiling and reviewed on every `cell_loader` refactor. The false "stays live via `CellRootRefIndex`" claim actively misdirects the next reader's deletion decision.

**Related**: #3455 (CLOSED — established the review rule this finding applies), #2369 / #2370 / #2372 (all CLOSED), #3833 (same "dead accessor kept for a future consumer" pattern, in the renderer)

**Suggested Fix**
Delete the three modules, both resource definitions and their two `boot.rs` insertions, plus the `mod` declarations in `cell_loader.rs`, the `CellRootRefIndex` row in `registry_completeness_tests.rs`, and the cross-references in `components.rs`. `World::find_by_form_id` / `resolve_entity_by_global_form_id` remain the live lookups. If a per-REFR index becomes necessary, the ~60-line `form_id_root_index::resolve` walk is trivially re-derivable from `git log`. If instead the team wants to keep it as scaffolding, the module docs must first be corrected — a new tracking issue must exist and the `CellRootRefIndex` escape-hatch sentence must be deleted, since it is untrue as written.

---

---

### TD8-2026-09-05-02: `load_interior_cell` is a dead `pub fn` behind a dead re-export — the same synchronous-superseded-by-resumable-job pattern as #2266/#3747, one file over


- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/cell_loader/transition.rs` (`load_interior_cell`, ~line 558–620), `byroredux/src/cell_loader.rs` (the `pub use transition::{ load_interior_cell, … }` block), `byroredux/src/cell_loader/load.rs` (an orphaned doc-comment cross-reference)
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: `a7cc9184`, 2026-05-21 ("M40 Phase 2 Stage 3b: interior↔exterior cell-swap orchestration")

**Description**
`load_interior_cell` is a bare `#[allow(dead_code)] pub fn` — no justifying comment at all, unlike every other allow in this file's neighbourhood. It performs a synchronous, unbudgeted interior cell load. It was superseded by `InteriorCellApply::begin(…)` + `advance(…)` in the same file, which is what `byroredux/src/app_step.rs` actually drives. `byroredux` is a **binary crate**, so `pub` here reaches nothing: there is no external consumer and never can be.

This is structurally identical to #2266/#3747 (`spawn_npc_entity` / `spawn_prebaked_npc_entity`): an older synchronous entry point left tagged `allow(dead_code)` when the resumable job API landed, kept alive only by doc comments that point at it.

**Evidence**
```
$ grep -RIn "load_interior_cell" --include="*.rs" crates byroredux tools
  byroredux/src/cell_loader.rs:91          #  the `pub use` re-export
  byroredux/src/cell_loader/transition.rs:379  #  doc comment "Used by [`load_interior_cell`] and …"
  byroredux/src/cell_loader/transition.rs:429  #  doc comment
  byroredux/src/cell_loader/transition.rs:559  #  the definition
  byroredux/src/cell_loader/load.rs:163        #  doc comment
  →  zero call sites
```
The live path, for contrast: `InteriorCellRequest` is consumed at `byroredux/src/app_step.rs:912`, which feeds `InteriorCellApply::begin` (`transition.rs`), not `load_interior_cell`.

**Impact**
~60 LOC of unreachable code plus three doc comments that describe it as a live caller of `reposition_camera` / `finish_interior_cell_load`, misrepresenting who actually drives those helpers. Any future change to the interior-load contract must be made twice or silently diverge.

**Related**: #2266 / #3747 (same pattern, CLOSED), TD8-2026-09-05-08 (the `allow(unused_imports)` blanket that hides the dead re-export)

**Suggested Fix**
Delete `load_interior_cell`, drop it from the `pub use transition::{…}` list, and reword the three doc comments to name `InteriorCellApply::begin` / `finish_interior_cell_load` — the functions that actually call the helpers those comments describe.

---

---

### TD8-2026-09-05-03: `crates/core/src/animation/controller.rs` (454 LOC) is a fully dead subsystem — nothing constructs `AnimationController` outside its own tests, and no system reads it


- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `crates/core/src/animation/controller.rs` (whole file: `AnimationController`, `ControllerTransition`, `ControllerTransitionDefaults`, `TransitionKind`, `apply_pending_transition`, `add_sequence`, `add_transition`, `request_sequence`, `set_sync_group`, `from_kfm_discriminant`), re-exported from `crates/core/src/animation/mod.rs`
- **Status**: NEW
- **Effort**: small (≤2 h) — the work is a keep-or-delete decision plus the mechanical removal
- **Age**: `07dc6b16`, 2026-04-23 ("Fix #338: add AnimationController — KFM-driven sequence state machine")

**Description**
The module presents itself as the glue that closes legacy-audit gap AR-09 / #338 — "the KFM parser provides catalog data, `AnimationStack` provides the blend mechanism, and this module connects them". Neither end was ever connected. `AnimationController` is a `Component`, but **no spawn path attaches it to any entity**, no system in `byroredux/src/systems/animation.rs` (or anywhere) queries it, and `apply_pending_transition` — its only entry point, publicly re-exported from `crates/core/src/animation/mod.rs` — has zero callers.

It is not a `#[allow(dead_code)]` case: the compiler cannot see it because every item is `pub` in a library crate.

**Evidence**
```
$ grep -RIn "AnimationController::new\|AnimationController {" --include="*.rs" crates byroredux tools
  crates/core/src/animation/controller.rs:37   # a ```text doc snippet (deliberately non-compiling, per #3348)
  crates/core/src/animation/controller.rs:140  # the struct definition
  crates/core/src/animation/controller.rs:301,307,314,324,331,339  # its own #[cfg(test)] mod

$ grep -RIn "apply_pending_transition" --include="*.rs" crates byroredux tools | grep -v controller.rs
  crates/core/src/animation/mod.rs:17          # the re-export, and nothing else

$ grep -RIn "AnimationController" --include="*.rs" crates/save byroredux/src | grep -v registry_completeness_tests
  →  (empty)   # not even save-registered; its only mention outside the crate
                #  is a rationale string in registry_completeness_tests.rs
$ grep -RIn "AnimationController" --include="*.rs" crates/nif/src
  crates/nif/src/kfm.rs:216,231,297            # three doc comments describing an
                                               #  integration that was never written
```

**Impact**
454 LOC of tested-but-unreachable state-machine code in `byroredux-core`, the most widely-depended-on crate in the workspace, plus three `crates/nif/src/kfm.rs` doc comments that tell a reader the KFM parser feeds a controller it has never fed. Any `AnimationStack` change must keep this parallel consumer compiling for no runtime benefit.

**Related**: #338 (the legacy-audit gap it claimed to close), TD8-2026-09-05-01 (same "shipped ahead of a consumer that never arrived" shape)

**Suggested Fix**
This is a judgement call, not a mechanical delete: either wire it (a KFM-driven actor needs `add_sequence` population at spawn and `apply_pending_transition` in the animation stage), or delete the module + its `animation/mod.rs` re-export + the `registry_completeness_tests.rs` row and reword the three `kfm.rs` doc comments to describe the KFM data as unconsumed. Given the project has no external consumers, "delete and re-derive from `git log` when an actor actually needs sequence blending" is the cheaper posture — but it should be an explicit decision, not an audit default.

---

---

### TD8-2026-09-05-04: `SkyParamsRes::texture_indices`'s `#[allow(dead_code)]` is stale — it has a production caller, and the 5-line justification above it is false


- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/components.rs` (`SkyParamsRes::texture_indices`, allow at line 1305)
- **Status**: NEW (regression of the class closed as #1632 / #1633)
- **Effort**: trivial (≤30 min)

**Description**
The attribute reads:
```rust
#[allow(dead_code)] // release hook not yet built; #1199 is the change that made this worldspace-scoped, not an open gate
pub(crate) fn texture_indices(&self) -> [u32; 5] {
```
and the doc comment above it says *"The matching release will live in a future worldspace-transition hook (door-walking interior↔exterior)"*, with a #3455 sibling-sweep note instructing the reader to **"File one [a tracking issue] before treating this as land-ahead-of-consumer rather than plain dead code."**

The release hook exists. `byroredux/src/scene/world_setup.rs` calls it in production, inside `apply_worldspace_weather`'s prologue:
```rust
let prev_sky_textures = world
    .try_resource::<SkyParamsRes>()
    .map(|s| s.texture_indices());
```
— the `#1339 / #1770` acquire-new-then-release-old handoff on worldspace change, i.e. exactly the hook the doc says is unbuilt. The call is at `world_setup.rs:261`; the file's only `#[cfg(test)]` starts at line 1092, so it is production, not a test.

Rust does not warn about a redundant `#[allow]` by default, which is why this rots silently and why the same class recurs (#1632, #1633, #2981, #1761 — all CLOSED).

**Evidence**
```
$ grep -RIn "texture_indices" --include="*.rs" byroredux/src
  byroredux/src/components.rs:1306                 # definition (with the stale allow)
  byroredux/src/scene/world_setup.rs:196           # doc comment naming it
  byroredux/src/scene/world_setup.rs:261           # THE PRODUCTION CALL
  byroredux/src/cell_loader/sky_params_cleanup_tests.rs:8,10,11,41   # the guard test
$ grep -n "#\[cfg(test)\]" byroredux/src/scene/world_setup.rs
  1092                                             # → line 261 is production
```

**Impact**
A reader auditing sky-texture lifetime is told the release hook does not exist when it does, and is instructed to open a tracking issue for work already shipped. Every future Dim 8 sweep re-triages this attribute from scratch.

**Related**: #1632, #1633, #2981, #1761 (same class, all CLOSED), #1199 / #1339 / #1770 (the changes involved)

**Suggested Fix**
Delete the `#[allow(dead_code)]` and rewrite the doc comment to name `scene/world_setup.rs`'s `apply_worldspace_weather` prologue as the live consumer. Consider enabling `clippy::allow_attributes_without_reason` or a periodic `-W unused` sweep so this class stops recurring.

---

---

### TD8-2026-09-05-05: `ActionState::was_released`'s `#[cfg_attr(not(test), allow(dead_code))]` is stale — `extensions.rs` calls it in production


- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/interaction.rs` (`ActionState::was_released`, allow at line 691)
- **Status**: NEW — regression of **#2981** (`ActionState::is_held`'s test-only allow was redundant; CLOSED), on the sibling method
- **Effort**: trivial (≤30 min)

**Description**
`was_released` sits between `is_held` and `was_pressed`, both of which carry no allow. It gained a production consumer when the SDK/mod-runtime event adapter landed: `byroredux/src/extensions.rs` uses it inside the `InputAction::OBSERVABLE` fan-out that builds `InputActionEvent`s for sandboxed mods:

```rust
let phase = if state.was_pressed(action) {
    InputPhase::Pressed
} else if state.was_released(action) {      // ← extensions.rs:4643, production
    InputPhase::Released
} else { return None; };
```

`extensions.rs`'s `#[cfg(test)]` block spans lines 5930–10652 (verified by brace-depth walk), so line 4643 is production code. The attribute is now a no-op that misdocuments the method as test-only.

**Evidence**
```
$ grep -RIn "was_released" --include="*.rs" byroredux crates tools
  byroredux/src/interaction.rs:692         # definition
  byroredux/src/interaction.rs:1189,1222,1248,1397   # tests (file's cfg(test))
  byroredux/src/commands_tests.rs:118      # test
  byroredux/src/extensions.rs:4643         # PRODUCTION  (cfg(test) mod starts at 5930)
```
Counter-check on its neighbour, so the finding is not over-broad: `ActionBindings::bind_key` (`interaction.rs:180`) carries the same attribute and its only non-`interaction.rs` caller, `extensions.rs:9451`, **is** inside the `cfg(test)` span — that attribute is correct and must stay.

**Impact**
Cosmetic in isolation, but this is the fourth recurrence of the same class in this file's neighbourhood (#2732, #2981, #1632, #1633). Each one costs a future auditor a full call-site trace to disprove.

**Related**: #2981 (the `is_held` twin, CLOSED), #2732 (four allows added to `interaction.rs`, CLOSED), TD8-2026-09-05-04

**Suggested Fix**
Delete the `#[cfg_attr(not(test), allow(dead_code))]` on `was_released`. Leave `bind_key`'s in place.

---

---

### TD8-2026-09-05-06: `MaterialProvider::register_starfield_cdb` is a test-only duplicate of the shipped CDB registration path, and its doc names a production caller that calls a different method


- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/asset_provider/material.rs` (`register_starfield_cdb`, allow at line 598; `discover_starfield_cdbs`; `register_starfield_cdb_probe`)
- **Status**: NEW
- **Effort**: trivial (≤30 min) to small (≤2 h, if the `peek_magic` reject is restored to the live path)

**Description**
`register_starfield_cdb(&mut self, bytes: &[u8])` opens with:
```rust
/// Validate + register a Starfield `materialsbeta.cdb` payload for the
/// presence gate — `discover_starfield_cdbs` calls this once per CDB
/// found across the loaded archives (#1571).
```
`discover_starfield_cdbs` does not call it. It extracts the payload, calls `ComponentDatabaseFile::probe_header` itself, and then calls the *private* `register_starfield_cdb_probe(info)` — the one-line `self.sf_cdb_count += 1` sibling. `register_starfield_cdb` is reached only from `byroredux/src/asset_provider/tests/starfield_mat.rs` (8 call sites).

Two consequences follow:

1. **The eight `starfield_mat.rs` tests exercise a parallel copy of the registration path, not the shipped one.** They validate `peek_magic` rejection, `probe_header` failure logging and the count increment through a function production never executes.
2. **The `peek_magic` cheap-reject added by SF-D3-AUDIT-03 / #2102 exists only in the dead copy.** `discover_starfield_cdbs` goes straight to `probe_header`. This is a lost micro-optimisation rather than a correctness gap — `probe_header` → `Parser::parse_header` validates the `BETH` signature anyway — but the two paths also emit *different* diagnostics for the same malformed input, so a real-world CDB rejection logs a different message than every test asserts against.

**Evidence**
```
$ sed -n '177,225p' byroredux/src/asset_provider/material.rs   # discover_starfield_cdbs
        …
        let probe = ComponentDatabaseFile::probe_header(&raw).ok();   # no peek_magic
        …
        if let Some(info) = probe { provider.register_starfield_cdb_probe(info); }

$ grep -RIn "register_starfield_cdb\b" --include="*.rs" byroredux crates
  byroredux/src/asset_provider/material.rs:599                    # definition
  byroredux/src/asset_provider/tests/starfield_mat.rs: 81,82,92,107,170,314,330,358   # 8 test calls
  →  no production caller
```

**Impact**
The test suite's coverage of Starfield CDB presence detection is a fiction: it can stay green while `discover_starfield_cdbs` regresses. Low blast radius today (the gate is presence-only, Phase 1), but Phase 2's per-field CDB index will be built on top of this path.

**Related**: #1571, #2100 (SF-D3-AUDIT-01, `probe_header`), #2102 (SF-D3-AUDIT-03, `peek_magic`), memory note "Starfield CDB Phase 2 Unblocked"

**Suggested Fix**
Delete `register_starfield_cdb` and repoint the eight tests at `discover_starfield_cdbs` (which already has an in-memory BA2 fixture builder — `starfield_mat.rs:258` references one). If the `peek_magic` fast reject is worth keeping, move it into `discover_starfield_cdbs` before the `probe_header` call rather than leaving it stranded in a dead function.

---

---

### TD8-2026-09-05-07: Three unused dependencies across three manifests — a fresh crop of the #2426–#2431 / #2075 class


- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/Cargo.toml` (`toml`), `crates/ui/Cargo.toml` (`image`), `tools/byro-detect/Cargo.toml` (`byroredux-core`, `toml`)
- **Status**: NEW (four prior sweeps of this class all CLOSED: #2075, #2426, #2427, #2428, #2429, #2430, #2431)
- **Effort**: trivial (≤30 min)

**Description**
`cargo machete` reports three manifests with declared-but-unreferenced dependencies. All three verified by hand — `cargo machete` misses macro-only and re-export usage, so each was re-checked with a `use`/path grep over the whole crate, tests and examples included:

```
$ grep -RInE '(^|[^a-z_])toml::|use toml|extern crate toml' byroredux/       → (empty)
$ grep -RInE '(^|[^a-z_])image::|use image|extern crate image' crates/ui/    → (empty)
$ grep -RInE 'byroredux_core|toml' tools/byro-detect/src/
  tools/byro-detect/src/main.rs:144:    home.join(".byroredux").join("profiles.toml")   # a filename string, not the crate
```

`tools/byro-detect` is the more interesting one: it builds a path to `~/.byroredux/profiles.toml` but never parses or writes it, so both `toml` and `byroredux-core` are declared against an intention rather than a use. `crates/ui`'s `image` is pinned with `default-features = false`, which suggests it was once used for the offscreen wgpu pixel readback before that moved to raw buffers.

**Impact**
`byroredux-core` in `byro-detect` is the costly one — it drags the whole ECS/animation/CHARAL crate into the launcher-detection binary's dependency graph and every CI build of it. `image` and `toml` are smaller but still compile-time-only cost for zero benefit. Nothing breaks; this is pure build-time waste plus a misleading signal about what the launcher tools actually depend on.

**Related**: #2075, #2426–#2431 (all CLOSED — same class, showing it recurs about every 3 months and needs a CI gate, not another sweep)

**Suggested Fix**
Remove all four declarations. Given this is the fifth occurrence, consider adding `cargo machete` to CI (it exits non-zero on findings and runs in seconds) so the class stops re-accruing between audits.

---

---

### TD8-2026-09-05-08: Seven production `#[allow(unused_imports)]` on `cell_loader.rs`'s re-export blocks suppress the compiler's only dead-re-export detector — in a binary crate that has no external API surface to protect


- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/cell_loader.rs` — the `pub use transition::{…}`, `pub(crate) use nif_import_registry::{…}`, `pub(crate) use refr::{…}`, `pub use exterior::{…}`, `pub(crate) use load::resolve_cell_lighting`, `pub use load::{…}`, `pub(crate) use object_lod::{…}` blocks
- **Status**: NEW
- **Effort**: small (≤2 h)

**Description**
`cell_loader.rs` carries 16 `#[allow(unused_imports)]` attributes. Eight are `#[cfg(test)]`-paired (test-visibility imports for child `mod`s — out of scope per the skill's cfg(test) exclusion), one more is a prose mention inside a comment. The remaining **seven sit on production re-export blocks** (`transition`, `nif_import_registry`, `refr`, `exterior`, `load::resolve_cell_lighting`, `load::{…}`, `object_lod`), justified by:

> `// Public re-exports — keep the existing crate::cell_loader::FOO call sites`
> `// … #[allow(unused_imports)] because not every re-exported item is consumed`
> `// by this crate's own binary — several only show up in external crates`
> `// (tests, other workspace members) or as the public API surface.`

`byroredux` is a **binary crate**. It has no external crates and no public API surface — the justification's second and third clauses cannot be true, and the first ("tests") is what the eight `cfg(test)` allows already cover. The net effect is that the one lint that can detect a dead re-export is switched off across most of the module's export surface.

Auditing what it hides: of the 37 names re-exported through those seven blocks, **three are dead re-exports** (referenced nowhere outside their defining module, in production or test):

| Re-exported name | Defining module | Consumers outside it |
|---|---|---|
| `load_interior_cell` | `transition.rs` | none — and the function itself is dead (TD8-2026-09-05-02) |
| `CellLoadPhaseTimings` | `load.rs` | none |
| `OneCellLoadInfo` | `exterior.rs` | none |

The rest are live and must stay. Two near-misses worth recording so a future sweep does not over-delete: `QueuedDoorTransition` / `QueueDoorTransitionError` appear at no call site by name but are the `Result` type of the live `queue_door_transition`, and `resolve_cell_lighting`'s re-export **is** load-bearing — `cell_loader/lgtm_fallback_tests.rs` reaches it through `use super::*`.

**Evidence**
```
$ grep -c '#\[allow(unused_imports)\]' byroredux/src/cell_loader.rs   → 16
# brace-walk pairing: 8 immediately preceded by #[cfg(test)], 1 is a prose mention,
# 7 sit on production re-export blocks (lines 89, 109, 113, 125, 130, 132, 144)

# per-name consumer count, excluding the defining module, cell_loader.rs itself, and test files:
  0  CellLoadPhaseTimings      0  OneCellLoadInfo      0  load_interior_cell
  1+ every other re-exported name
```

**Impact**
Structural, not cosmetic: this is *how* `load_interior_cell` survived from 2026-05-21 to today without a single compiler complaint, and it will hide the next one identically. Three dead re-exports today, unbounded tomorrow.

**Related**: TD8-2026-09-05-02 (the dead function this blanket concealed), #1322 / #2431 (dead re-exports found by hand in other crates, both CLOSED)

**Suggested Fix**
Delete the three dead re-exports, then remove the seven production `#[allow(unused_imports)]` attributes and let `cargo check` name whatever is left; re-add narrowly (per-name, with a reason) only where the compiler actually complains and the name is genuinely needed for a child test module's `use super::*`. Correct the justifying comment: it describes a library crate, and this is a binary.

---

---

### TD8-2026-09-05-09: `QuestStageState`'s four dynamic-subscription methods are superseded by three static subscriber-ID constants and survive only on their own unit tests


- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `crates/scripting/src/quest_stages.rs` — `subscribe_to_quest_events`, `subscribe_to_retained_quest_events`, `acknowledge_quest_events`, `unsubscribe_from_quest_events` (lines 334–361), plus the private `QuestEventLog::subscribe` / `acknowledge` and the `next_subscriber_id` counter behind them
- **Status**: NEW
- **Effort**: trivial (≤30 min) to small (≤2 h, depending on how much of `QuestEventLog`'s dynamic half goes with them)

**Description**
The quest-event log was designed with dynamic subscriber registration (`subscribe → poll → acknowledge → unsubscribe`). Production settled on a different model: three compile-time constants, and every caller passes one directly to `poll_quest_events`.

```rust
pub const SCENE_QUEST_EVENT_SUBSCRIBER:    QuestEventSubscriberId = QuestEventSubscriberId(1);
pub const FRAGMENT_QUEST_EVENT_SUBSCRIBER: QuestEventSubscriberId = QuestEventSubscriberId(2);
pub const TERMINAL_QUEST_EVENT_SUBSCRIBER: QuestEventSubscriberId = QuestEventSubscriberId(3);
```

All three production consumers — `crates/scripting/src/scene/playback.rs`, `crates/scripting/src/fragment.rs` (×2), and `quest_stages.rs`'s own terminal system — use a constant. `subscribe_*` / `unsubscribe_*` / `acknowledge_*` have **zero production callers anywhere in the workspace**; their only callers are five sites inside this file's own `#[cfg(test)]` mod (which starts at line 1269).

`acknowledge_quest_events` is the sharpest instance, because its doc comment asserts a caller that does not exist: *"Fragment dispatch uses this after synchronously cascading its own SetStage calls so those same transitions are not dispatched again on its next cadence."* `fragment.rs` calls only `poll_quest_events`.

**Evidence**
```
$ grep -RIn "subscribe_to_quest_events\|subscribe_to_retained_quest_events\|unsubscribe_from_quest_events\|acknowledge_quest_events" \
      --include="*.rs" crates byroredux tools
  crates/scripting/src/quest_stages.rs:334,340,355,359          # the four definitions
  crates/scripting/src/quest_stages.rs:1497,1498,1535,1572,1604 # its own cfg(test) mod (starts 1269)
  →  no other file in the workspace mentions any of them

$ grep -RIn "poll_quest_events" --include="*.rs" crates byroredux
  crates/scripting/src/scene/playback.rs:494   → SCENE_QUEST_EVENT_SUBSCRIBER
  crates/scripting/src/fragment.rs:722, 2454   → FRAGMENT_QUEST_EVENT_SUBSCRIBER
  crates/scripting/src/quest_stages.rs:1171    → TERMINAL_QUEST_EVENT_SUBSCRIBER
```

**Impact**
Two competing subscriber-lifecycle models coexist in one type. A reader adding a fourth quest-event consumer has to work out which is canonical, and the tests actively suggest the wrong one (they use `subscribe_*`, production does not). `acknowledge_quest_events`'s doc describes fragment-dispatch behaviour that is not implemented, so a reader debugging duplicate stage dispatch is pointed at a mechanism that never runs.

**Related**: #2727 (catalog-drift guard only reachable via `#[ignore]`, same "tested path ≠ shipped path" shape, CLOSED), TD8-2026-09-05-06 (identical shape in the Starfield CDB provider)

**Suggested Fix**
Either delete the four methods and the dynamic half of `QuestEventLog` (rewriting the five tests to use the three constants, which is what they should be exercising), or — if dynamic subscription is genuinely wanted for mod-supplied consumers — wire at least one production caller and delete the static constants. Do not leave both. At minimum, correct `acknowledge_quest_events`'s doc comment, which currently asserts a caller that does not exist.

---

## Noted, not filed

Per the skill's exclusions (`cfg(test)` / `cfg(debug_assertions)`-gated code, FFI boundaries, and workspace-internal public API a future binary will consume — *note, don't delete*):

| Item | Why not a finding |
|---|---|
| `byroredux/src/groundcover_translate.rs` — 5 allows (`DEFAULT_AFFINITY`, `SUPPRESSION_KEYWORDS`, `AFFINITY_KEYWORDS`, `layer_affinity`, `layer_affinities`) | Genuine land-ahead-of-consumer, and **still valid**: the named consumer, a `groundcover_scatter.comp` Phase-1 dispatch, does not exist (`ls crates/renderer/shaders/ \| grep -i ground` → empty; no `cover_affinity` reference in any shader). The palette/wind half of the module *is* live via `install_ground_cover` (`scene/world_setup.rs`). Fully exercised against the real 386-record `LTEX` corpus. Re-check when Phase 1 lands. |
| `crates/plugin/src/legacy/` — file-level `#![allow(dead_code)]`, 358 LOC | Documented ESM/ESP/ESL/ESH FormId-bridge scaffolding, narrowed to `pub(crate)` by #1322 (CLOSED) precisely so it stays off the API surface. Its module doc states the gating condition plainly ("the plumbing … will land alongside its first real consumer"). No milestone claim to expire, so unlike TD8-01 there is nothing stale to check. |
| `crates/hkx/src/packfile.rs::global_target` | Deliberately kept and test-exercised per #2267 (CLOSED); the decision and its rationale are written down at the site. |
| `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` — `VF_UVS_2`, `VF_LAND_DATA`, `VF_INSTANCE` | Schema-completeness constants for `BSVertexDesc` bits, backed by `check_vertex_desc_offsets`, which `log::warn!`s on drift. Deleting them would remove the drift check's vocabulary. Tracked under #336 / #2578. |
| `crates/nif/src/shader_flags.rs` — ~35 unreferenced `const`s | Same class: a complete transcription of the Bethesda shader-flag vocabulary. The set's value is completeness; a partial set is worse than a full one. |
| `crates/renderer/src/vulkan/material.rs` — `polished_metal`, `glass`, `car_paint`, `lacquered_plastic`, `painted_matte`, `skin_wax_marble` | The documented Disney preset reference table from `c09d63a6` / #1251, consumed only by its own tests. "Preset unused" was already observed and accepted under #1627 (CLOSED); re-filing would re-litigate a closed decision. |
| `crates/core/src/ecs/query.rs` — `QueryRead::guard`, `QueryWrite::guard`, `ComponentRef::guard` | RAII lock guards, structurally unreadable by design (the cached `storage` pointer's validity *is* the guard's purpose). Correct use of the attribute. |
| `crates/renderer/src/vulkan/scene_buffer/buffers.rs::LightHeader::count` | Write-only by design — byte-copied into the std430 SSBO header for the shader. Correct. |
| `crates/bsa/src/ba2.rs::Dx10Chunk::end_mip` | Reserved for partial-mip-range streaming, with `start_mip` already live. Prior triage under #1761 (CLOSED). Worth a Dim 5 re-check that its `MILESTONE: M40` marker has not outlived its driver — but the field itself is legitimately reserved, not rot. |
| `crates/plugin/examples/sf_smoke.rs::WalkReport::tes4_bytes` | Write-only struct field in an example target. Real but negligible; not worth an issue. |
| `crates/plugin/src/esm/records/grup_walker.rs` `unused_mut` (the one live compiler warning in the workspace) | Inside a `#[cfg(test)]` closure — excluded by the skill's cfg(test) rule. Worth folding into any nearby edit. |
| `crates/mod-runtime`, `crates/sdk` public surface | Consumer-less-by-design contract crates; `crates/mod-runtime`'s dangling workspace alias was already closed as #3748 and has not regressed (`byroredux/src/extensions.rs` is now a real consumer). Audited as contracts, not live paths, per `_audit-common.md`. |

---

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 9 |

**Effort**: 5 trivial (≤30 min), 4 small (≤2 h). No medium or large.

**By theme**: 3 findings are dead subsystems shipped ahead of consumers that never arrived (01, 03, and the deletion condition in 01 that is now met); 2 are stale `#[allow]` attributes whose justification is provably false (04, 05); 2 are functions kept alive only by their own tests while production took a different path (06, 09); 1 is a superseded synchronous entry point (02); 1 is build-graph waste (07); and 1 (08) is the *mechanism* — a blanket lint suppression in a binary crate — that let 02 survive four months undetected.

**Highest-value single fix**: TD8-2026-09-05-08, because it is causal rather than symptomatic — removing seven unjustifiable `#[allow(unused_imports)]` restores the compiler's own detection of the class that produced finding 02 and 08's three dead re-exports.

**Largest single cleanup**: TD8-2026-09-05-01 (~186 production LOC across 3 modules, 2 resources, 2 boot insertions, 8 `#[allow(dead_code)]`, and a false doc claim that would misdirect the next auditor's deletion).

---

### TD9-2026-09-05-03: Dim 9's own discovery recipe — and the Phase-1 baseline snapshot — are structurally blind to `tools/`

- **Severity**: LOW
- **Dimension**: Test Hygiene (Dim 9) — `tech-debt`
- **Location**: `/mnt/data/src/gamebyro-redux/.claude/commands/audit-tech-debt/SKILL.md` (the Dimension 9 **Discovery** block, and the Phase-1 *Snapshot totals* block)
- **Status**: NEW (fourth in the #2262 → #3440 → #3456 recipe-accuracy family)
- **Description**: Both recipes scope to `crates byroredux`. The workspace also has `tools/`, which is where `byro-dbg`, `byro-detect`, `byro-launcher` and `texture-upscale` live. `tools/byro-launcher/src/preflight.rs` carries `#[ignore = "needs a Vulkan driver"]`, so the published `#[ignore]` figure is **181 where the tree-wide figure is 182**. The same blind spot applies to every other Phase-1 metric computed with that path pair: TODO/FIXME markers, `allow(dead_code)`, `unimplemented!/todo!()`, and the >2000-LOC file scans.

  The 1-site undercount is immaterial today. The *structural* blindness is not: `tools/byro-launcher` and `tools/byro-detect` landed on 2026-08-30 (~5.8k LOC across them and their two backing crates), `_audit-common.md` lists the launcher stack as one of eight **un-owned subsystems** with no owner audit skill, and it is "the only path a non-developer reaches the engine through". A recipe that cannot see it means no audit dimension will ever report debt there by default.
- **Evidence**:
  ```
  grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux | wc -l   → 181
  grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux tools | wc -l → 182
  grep -RInE '^[[:space:]]*#\[ignore' --include='*.rs' tools
    → tools/byro-launcher/src/preflight.rs:268:    #[ignore = "needs a Vulkan driver"]
  ```
  `_audit-common.md` "Un-owned subsystems" table, row *Launcher (boot/settings/detect)*: `tools/byro-launcher/`, `tools/byro-detect/`, "No owner".
- **Impact**: Audit-infrastructure accuracy. Small today (0.55 % undercount); the cost is that `tools/` debt is invisible by construction, in a tree where four of the ten workspace binaries now live there. The three prior findings in this family each shipped a wrong number into a published audit report before being caught.
- **Related**: #2262, #3440, #3456, #3749 (all recipe/baseline-accuracy findings on this same grep); `_audit-common.md` un-owned-subsystems table.
- **Suggested Fix**: Change `crates byroredux` → `crates byroredux tools` in the Dimension 9 discovery block and in all six Phase-1 snapshot lines, and note the new tree-wide baseline (182) beside the historical one so the next audit's diff is not read as a regression. `tools/nifskope/` is vendored and not a workspace member — exclude it explicitly, as `_audit-common.md` already instructs.
- **Effort**: **Trivial** (two blocks in one skill file, plus one `_audit-validate.sh` run).

---

---

### TD9-2026-09-05-04: Five `recon`-gated `crates/spt` example binaries have no compile gate in any lane

- **Severity**: LOW
- **Dimension**: Test Hygiene (Dim 9) — `test-gap`
- **Location**: `/mnt/data/src/gamebyro-redux/crates/spt/Cargo.toml` (five `[[example]]` targets with `required-features = ["recon"]`), sources under `/mnt/data/src/gamebyro-redux/crates/spt/examples/`
- **Status**: NEW
- **Description**: `byroredux-spt` declares `default = []` and `recon = []`, and gates `spt_recon`, `spt_dissect`, `spt_tagmap`, `spt_transitions`, `spt_walk` behind `required-features = ["recon"]`. Cargo skips a target whose required features are off, and no CI job passes `--features recon` — so these five reverse-engineering harnesses are never type-checked. They are the tooling that produced the SpeedTree tag dictionary (`6f83b1c3`), i.e. the artifacts a future `.spt` investigation would reach for first.

  I verified they are **not currently broken**: `cargo check -p byroredux-spt --features recon --examples` finishes clean. This is therefore a hardening finding, not a live breakage — but nothing prevents the next `crates/spt` or `crates/bsa` API change from silently rotting them.

  The `recon` feature comment anticipates "the future format-discovery integration tests"; none exist yet, so no *test* is currently dark behind a never-enabled feature. That triage bullet is otherwise clean this cycle (see Verified Clean).
- **Evidence**: `crates/spt/Cargo.toml` lines declaring the five `[[example]]` blocks; `grep -rn 'features' .github/workflows/ci.yml` shows only `--features dhat-heap` (a dedicated job), never `recon`; `cargo check -p byroredux-spt --features recon --examples` → `Finished dev profile ... in 5.64s`.
- **Impact**: Silent bit-rot of dev tooling for an already-thinly-owned crate (`/audit-speedtree` owns the parser; the examples are effectively unowned). Discovered only when someone needs them, which is exactly when they are least welcome to fix.
- **Related**: `/audit-speedtree`; the `dhat-heap` job in `ci.yml` is the in-repo precedent for gating a non-default feature.
- **Suggested Fix**: Add `cargo check -p byroredux-spt --features recon --examples` to the existing `ci.yml` clippy/check job — one line, seconds of wall time, no new job.
- **Effort**: **Trivial**.

---

---

### TD9-2026-09-05-05: `cargo test -p byroredux-core` — the command CLAUDE.md documents for core tests — silently drops the two `inspect`-gated round-trip tests that only the workspace lane compiles

- **Severity**: LOW
- **Dimension**: Test Hygiene (Dim 9) — `test-gap`
- **Location**: `/mnt/data/src/gamebyro-redux/crates/core/src/animation/player.rs` (`inspect_tests::reverse_direction_round_trips_through_json`) and `/mnt/data/src/gamebyro-redux/crates/core/src/animation/stack.rs` (`inspect_tests::stack_round_trips_reverse_and_blend_state`); documented command in `/mnt/data/src/gamebyro-redux/CLAUDE.md` Quick Reference
- **Status**: NEW
- **Description**: Both modules are `#[cfg(all(test, feature = "inspect"))]`, and `byroredux-core`'s `default` is `["parallel-scheduler"]` only. They compile in CI purely by feature unification: `byroredux-save` depends on `byroredux-core` with `features = ["save"]`, and `save = ["inspect"]`, so `cargo test --workspace` builds core once with `inspect` on. **CI coverage is therefore fine** — this is the answer to the "feature-gated tests never enabled in CI" triage bullet, and I verified it empirically rather than by reasoning about the resolver.

  The gap is the *documented developer command*. `CLAUDE.md` tells contributors to run `cargo test -p byroredux-core` for core tests; that invocation does not pull `byroredux-save` in, so `inspect` stays off and both #486 serialization guards vanish from the run with no diagnostic. A contributor iterating on `AnimationPlayer`/`AnimationStack` locally gets a green that omits precisely the two tests covering the field they are editing.
- **Evidence**:
  ```
  cargo test -p byroredux-core --lib -- --list                    → 742 tests, 0 matches for inspect_tests
  cargo test -p byroredux-core -p byroredux-save --lib -- --list  → animation::player::inspect_tests::reverse_direction_round_trips_through_json
                                                                    animation::stack::inspect_tests::stack_round_trips_reverse_and_blend_state
  ```
  `crates/core/Cargo.toml`: `default = ["parallel-scheduler"]`, `inspect = [...]`, `save = ["inspect"]`; `crates/save/Cargo.toml`: `byroredux-core = { workspace = true, features = ["save"] }`.
- **Impact**: Local false confidence on the animation-serialization path only; CI still catches it before merge. Bounded.
- **Related**: #486 (the issue both tests guard). **Cross-dimension note for the merge step**: the same CLAUDE.md line also claims "(162 tests)" where the real lib figure is **742** — that number is pure doc rot and belongs to **Dimension 3**, not here; it is flagged rather than double-filed, per the Cross-Dimension Dedup rule.
- **Suggested Fix**: Change the Quick Reference line to `cargo test -p byroredux-core --features inspect` (or `-p byroredux-core -p byroredux-save`) so the documented command matches what CI actually exercises.
- **Effort**: **Trivial**.

---

# Verified Clean (checked, nothing to file)

These are recorded so the next audit does not re-derive them.

- **Bare `#[ignore]`: 0.** #3749's conversion to `#[ignore = "<reason>"]` is holding across all 182 sites, `tools/` included. No new test skipped the convention.
- **No `#[ignore]` guards a closed CRITICAL/HIGH fix.** Not one of the 181 in-scope reason strings references an issue number; all are device/data/memory gates. The severity table's MEDIUM trigger did not fire.
- **Commented-out assertions: 0.** `grep -rnE '^\s*//\s*assert(_eq|_ne)?!'` over `crates byroredux tools` returns nothing.
- **Assertion-free tests: 0 real.** A naive sweep flagged 47; refining it to follow calls into `assert_*` / `approx` / `check_*` helpers leaves 19, and every one of those 19 is a deliberate "does not panic" / "is a safe noop" test carrying an explanatory comment (e.g. `flat_walker_caps_at_max_depth`: *"Nothing to assert on the mesh side … success is 'returned without overflowing the stack'"*; `pod_marker_covers_every_instantiated_type` asserts by trait bound at compile time). **#2432 and #3083 have not regressed.**
- **Smoke-only assertions: 0 real.** One candidate, `growth_landing_exactly_at_hard_cap_is_allowed` (`crates/renderer/src/mesh.rs`), asserts `is_ok()` on a `Result<(), String>` — there is no value to assert on the success arm. False positive.
- **`println!`-without-assert: 0.** No `#[test]` in the tree prints without also asserting.
- **Feature-gated tests never enabled in CI: 0.** The two `inspect_tests` modules do run under `cargo test --workspace` (empirically confirmed); the five `recon` targets are examples, not tests (filed separately as -04); the `dhat-heap` heap-bound tests have a dedicated `ci.yml` job, so **#1763 is holding**.
- **Regression tests named in sibling audit skills: all present.** Cross-referencing every backticked `snake_case` symbol in `.claude/commands/**/*.md` against the 7 310-test index found **12** that are `#[ignore]`d — `clas_oblivion_knight_against_vanilla`, `race_oblivion_data_and_subs_against_vanilla`, `cross_game_translation_completeness`, `da10_pex_reproduces_hand_builder_byte_for_byte`, `fnv_protectron_skeleton_is_one_connected_component`, `looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`, `non_looping_emitter_stops_on_emitter_remove_regression_858`, `parse_rate_fallout_4`, `parse_rate_fo4_all_meshes`, `parse_rate_starfield`, `parse_real_skyrim_esm`, `real_archive_torch_meshes_surface_particle_emitters` — and every one is gated on game data or an audio device, i.e. legitimate. **None** is a disabled regression guard. (They are, however, all in the TD9-…-02 soft-skip population, which is the point of that finding.)
- **The four `--ignored` corpus tests gated by `bdc0d84e`** (peak test-run RSS 2828 → 168 MB) are still gated, so the #3843/plugin-OOM class recorded in auto-memory has not re-opened.

---

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 3 |
| **Total** | **5** |

Effort: 3 × Trivial, 1 × Small, 1 × Medium.

---

## Scope Covered & Verified-Clean (per dimension)

Each dimension recorded what it checked and found clean. Kept so the next
sweep does not re-derive the same negatives.

### Dimension 1 — File / Function / Module Complexity

**Audit**: `/audit-tech-debt` · **Date**: 2026-09-05 · **Repo HEAD**: `fa5c4191`
**Scope**: the 8 UNTRACKED files in the primary (production-LOC > 2000) bucket, plus the
dimension's secondary flags (functions > 200 LOC, match arms > 50, nesting depth > 5,
`mod.rs`/`lib.rs` with > 20 `pub use`).

## Bucket state (re-verified with `prod_loc`, 2026-09-05)

| # | prod | total | file | status |
|---|---|---|---|---|
| 1 | 5921 | 10652 | `byroredux/src/extensions.rs` | **NEW** → TD1-…-01 |
| 2 | 3759 | 5344 | `crates/sdk/src/compatibility.rs` | **NEW** → TD1-…-02 |
| 3 | 3711 | 6158 | `crates/scripting/src/papyrus_provider.rs` | **NEW** → TD1-…-03 |
| 4 | 3495 | 4588 | `crates/mod-runtime/src/runtime.rs` | **NEW** → TD1-…-04 |
| 5 | 2937 | 3863 | `crates/renderer/src/vulkan/volumetrics.rs` | Existing **#2256** (escalated, TD1-…-11) |
| 6 | 2645 | 3280 | `crates/renderer/src/vulkan/context/mod.rs` | Existing **#3736** |
| 7 | 2538 | 2540 | `crates/scripting/src/fragment.rs` | **NEW** → TD1-…-05 |
| 8 | 2232 | 2670 | `byroredux/src/boot.rs` | **NEW** → TD1-…-06 |
| 9 | 2230 | 3256 | `crates/renderer/src/mesh.rs` | Existing **#3451** |
| 10 | 2165 | 2167 | `crates/nif/src/import/walk/mod.rs` | **NEW** → TD1-…-07 |
| 11 | 2063 | 2071 | `crates/renderer/src/texture_registry.rs` | Existing **#3737** |
| 12 | 2044 | 2044 | `byroredux/src/asset_provider/material.rs` | **NEW** → TD1-…-08 |

Secondary bucket (total LOC > 2000): **40 files**. Every member was re-measured with
`prod_loc`; the 12 above are exactly the members whose production half also crosses 2000.
**No secondary-bucket escalations.** Six members report 0 production (pure test files reached
through an external `#[cfg(test)] mod` declaration): `scene_buffer/shader_contract_tests.rs`,
`plugin/tests/parse_real_esm.rs`, `scripting/src/fragment/tests.rs`, `mod-runtime/src/tests.rs`,
`plugin/src/esm/records/tests.rs`, `asset_provider/tests/bgsm_merge.rs`, `npc_spawn/tests.rs`.

**Repo-wide production functions > 200 LOC: 133** (excluding `*tests.rs` and `tests/`).
22 of those live inside the 8 files below; the rest are triaged in TD1-…-09.

---

### Dimension 2 — Logic Duplication

**Audit**: `/audit-tech-debt` · **Date**: 2026-09-05 · **Depth**: deep
**Dedup baseline**: `/tmp/audit/tech-debt/issues_all.json` (500 tech-debt issues, 12 open)
+ `docs/audits/AUDIT_TECH_DEBT_2026-08-{16,20,24,27,30}.md`.

**Scope**: the four discovery targets named in the mandate
(`crates/nif/src/blocks/`, `crates/renderer/src/vulkan/`,
`crates/plugin/src/esm/records/`, `byroredux/src/cell_loader/`), widened by a
normalised 12–14-line sliding-window duplicate scan across `crates/` and
`byroredux/` so the young, never-swept crates (`crates/sdk`,
`crates/mod-runtime`, `crates/scripting`, and `byroredux/src/extensions.rs`)
were actually reached rather than assumed clean.

**8 findings** — 2 MEDIUM, 6 LOW. Every finding names a concrete
consolidation site.

---

## Verified clean — do NOT re-file these

Recorded because three of them are named in the discovery recipe and would
otherwise be re-derived as findings every cycle:

| Candidate | Live state |
|---|---|
| Z-up → Y-up coordinate flip | **Converged.** `crates/core/src/math/coord.rs` is the single source of truth; `crates/nif/src/anim/coord.rs` is a 14-line `pub use` whose own header documents the #1044/TD3-002 collapse. 49 files reference the re-exports; none reimplements the swizzle. Flagging those call sites would invert the truth. |
| `vk::WriteDescriptorSet` boilerplate | **Converged.** `WriteDescriptorSet::default()` appears **0** times outside `crates/renderer/src/vulkan/descriptors.rs`; all 135 descriptor writes route through `write_combined_image_sampler` / `write_storage_image` / `write_storage_buffer` / `write_uniform_buffer` / `write_acceleration_structure`. #2073 / #1752 held. |
| Compute-pipeline creation | **Converged.** One `create_compute_pipelines` call in the whole crate, inside `pipeline.rs::create_compute_pipeline`. #1751 / #2072 held. |
| BC1/BC3/BC5/RGBA texture-upload chain | **Converged.** `cmd_copy_buffer_to_image` appears in exactly two files — `texture.rs` (`Texture::record_dds_upload`, the shared chain) and `volumetrics.rs` (a 3D noise volume, a different shape). No per-format duplication. |
| ESM `EDID`/`FULL`/`MODL` sub-record bundle | **Converged.** 54 `CommonNamedFields::from_subs` call sites. The 12 surviving hand-written `b"EDID" =>` arms are single-field reads in records that carry no FULL/MODL bundle (weather, climate, global, script, container) plus the CELL/WRLD walkers. #2414 / #2068 held. |

---

### Dimension 3 — Stale Documentation & Comments

**Audit**: `/audit-tech-debt` · **Date**: 2026-09-05 · **HEAD**: `6fba2b0a` (+ today's `fa5c4191`)

## Scope & Method

The path gate (`.claude/commands/_audit-validate.sh`) was already run at Phase 1 and
reports `OK: all path references valid` — re-confirmed in this dimension. So there are
**zero** STALE-path findings; everything below is content rot the gate cannot see.

Ground truth was taken from the live pins, never from prose:

| Struct | Live size | Pin |
|---|---|---|
| `GpuInstance` | **160 B** | `gpu_instance_is_160_bytes_std430_compatible` (`gpu_instance_layout_tests.rs`) |
| `GpuCamera` | **368 B** | `gpu_camera_is_368_bytes` (same file) |
| `GpuMaterial` | **432 B** | `gpu_material_size_is_432_bytes` (`vulkan/material_tests.rs`) |
| `GpuLight` | **64 B** | `gpu_light_is_64_bytes` |
| `GpuTerrainTile` | **96 B** | `gpu_terrain_tile_is_96_bytes` |
| `Vertex` | **104 B** | `assert_eq!(size_of::<Vertex>(), 104)` (`crates/renderer/src/vertex.rs`) |

Dated reports under `docs/audits/` were treated as point-in-time snapshots, **not** rot —
they legitimately record what was true on their own date. Only living docs, skills,
in-code doc comments and shader comments were swept.

**Deduped against** `/tmp/audit/tech-debt/issues_all.json` (500 tech-debt issues, 12 open)
plus targeted `gh issue view` on the near-matches (#1755, #1624). **Not re-filed** per
instruction: #3830, #3831, #3832, #3841, #3842.

### Verified CLEAN (checked, no finding)

- `docs/engine/shader-pipeline.md` — `GpuInstance` (160 B), `GpuCamera` (368 B),
  `GpuMaterial` (432 B), `GpuLight` (64 B) headings and the full `GpuInstance`
  offset table (0…148, incl. the three `_reserved2*` scalars) match the live struct
  field-for-field.
- `docs/engine/renderer.md` — the 112→160 / 300→432 / 368 B size narratives all
  terminate on the live value.
- `crates/renderer/shaders/volumetrics_inject.comp` — `GpuBoundaryInstance`'s comment
  block was correctly rewritten to 160 B by today's `fa5c4191` (`_boundaryTail1` /
  `_boundaryTail2`, "144..160"). No rot.
- `crates/renderer/shaders/triangle.frag` — carries **no** stale GPU-struct byte size
  (the dimension's named canonical trap for this file is clean; the rot found there is
  a different class, TD3-2026-09-05-04).
- `crates/core/src/ecs/components/material.rs` — all 5 `classify_pbr` doc mentions
  correctly frame it as `(deleted)` / `(deleted per-draw)` / "removed in". The recurring
  trap is **clean in the file the skill names**; the surviving site is elsewhere
  (TD3-2026-09-05-05).
- `README.md` — every `--flag` in every command example resolves to a live handler
  (`cli_args.rs` / `boot.rs` / `scene.rs` / `extensions.rs`), including the
  `texture-upscale discover --source/--manifest` subcommand, which `main.rs` really
  does dispatch into `byro_texture_upscale::run_cli_from`.
- `ROADMAP.md` — no milestone is marked "in progress"/WIP/🚧 at all; the status
  vocabulary is `Closed <date>` + open-tracker prose. No open-vs-closed inversion found.
- `HISTORY.md` — no `Revert` commits exist in `git log`; the four orphan-branch closed
  issues (#2266/#3084/#3170/#3169) are not claimed as landed in HISTORY.md or ROADMAP.md.
- `docs/feature-matrix.md` — 14 rows spot-checked against their implementing crate
  (hkx/Skyrim-only, 7-of-17 PACK procedures, CHARAL wiring, M47.2 `.pex` slice, M45
  save continuity, native menu). Only one row drifted (TD3-2026-09-05-03).

### Noted, not filed (belongs to Dim 4)

`_audit-validate.sh`'s symbol advisory reports **240** backticked symbols in
`docs/engine/` that exist in no tracked `.rs` file. Spot-checking the highest-signal
NIFAL/EXAL/UI entries (`translate_node`, `translate_sun`, `translate_cell_lighting`,
`update_ui_texture`) shows they are all *deliberately* non-existent — the surrounding
sentences read "there is no single `translate_node` boundary", "A future `translate_sun`
(step 4) will fold…", "There is no bespoke `update_ui_texture` entry point". These are
backtick-convention violations (the convention says italicise a not-yet-built or
deliberately-absent name), not content rot, and the advisory's own noise floor is
dominated by GMST / ESM-field / wiki-sourced names. Routed to Dim 4 (audit-infrastructure)
rather than filed here.

---

## Findings

### Dimension 4 — Audit-Finding Rot

**Audit**: `/audit-tech-debt` · **Date**: 2026-09-05 · **HEAD**: `6fba2b0a`
**Scope**: `.claude/commands/_audit-*.md` (2 files), `.claude/commands/audit-*/SKILL.md`
(28 files), `.claude/commands/_audit-validate.sh`, `docs/audits/` (629 reports).
**Dedup baseline**: `/tmp/audit/tech-debt/issues_all.json` (500 tech-debt issues, 12 open)
plus a full 3,730-issue pull for cross-label matching.

---

## Method / gate state

`.claude/commands/_audit-validate.sh` is **GREEN**: `OK: all path references valid`,
2,450 refs across 102 files, crate-count guard in sync (28), NUL-byte guard clean.

Two advisory sections, triaged separately below:

| Advisory corpus | Count | Verdict |
|---|---|---|
| audit skills | 4 | All false positives — `concurrency`, `enhancement`, `speedtree` are GitHub **label names** and `Related` is a **finding-format field**, none of which are or should be repo symbols. No finding. |
| `docs/engine` reference docs | 240 | Mixed. ~13 are real backtick-convention violations (TD4-2026-09-05-04); the rest is the documented noise floor (Papyrus event names, GMST/perk rosters, nif.xml fields, on-disk format fields). |

---

## Findings

### Dimension 5 — Stale Markers

**Audit**: `/audit-tech-debt` · **Date**: 2026-09-05 · **Depth**: deep (every hit
triaged individually) · **Repo**: `/mnt/data/src/gamebyro-redux` @ `6fba2b0a`

## Headline

**Zero live markers. Fourth consecutive clean run** (2026-08-16, 08-27, 08-30,
09-05). The recipe returns 20 hits in `crates`+`byroredux` and **0** in
`crates/renderer/shaders/`; all 20 fall into the documented exclusion classes,
and the composition is byte-for-byte unchanged since 2026-08-16. Not one hit is
older than 6 months (oldest: `bgem.rs:137`, 4.5 months), so the ">6 months →
report" rule fires on nothing. No marker names an open *or* closed issue as a
live driver — the four issue numbers that do appear (`#242`, `#1055`, `#1347`,
`#3324`) are all in closure/history framing, all CLOSED, and all verified
accurate against current code.

The MUST-NOT-DELETE check passes: the third-party attribution block atop
`crates/renderer/shaders/triangle.frag` is intact — GLSL-PathTracer MIT notice
(Asif Ali, 2019) *and* the Burley 2012 Disney-BSDF citation, both present with
the "MIT requires this notice travel with the code" line and the
`search "GLSL-PathTracer"` pointer to the per-function inline citations.

The two findings below are **not** live markers — they are gaps in this
dimension's own discovery recipe, found by deliberately over-scanning past the
recipe's boundaries. Precedent for filing them: **#3456** (CLOSED) did exactly
this for Dimension 9's `#[ignore = "reason"]` blind spot, and **#2974** for
Dimension 1's total-LOC proxy.

### Cross-checks run (no findings, recorded so the next audit can skip them)

- `unimplemented!` / `todo!(` across `crates`+`byroredux`: **0** — matches the
  skill's stated baseline (Dim 6's subject; recorded here only as corroboration
  that no marker migrated into a panic).
- Case-insensitive `\b(todo|fixme|hack|xxx)\b` over all `.rs`: adds 3 sites, all
  English prose (`"translation hack"`, `"successful hack"`, `"a to-do list"`).
- Alternative spellings (`@todo`, `FIX ME`, `TO-DO`, `HACK-`, `KLUDGE`, `WIP`):
  **0** beyond the prose above.
- `#2263` (CLOSED, "XXXX exclusion list doesn't name the newest reference
  sites") — **fix holding**: the skill now keys the exclusion on comment
  *content*, not a file list, which is why today's two newest `XXXX` sites
  (`wrld.rs`, `magic.rs`) triaged correctly with no skill edit.
- `mktemp` `XXXXXX` templates in `scripts/*.sh` (4 sites) — same false-positive
  class as the ESM tag; out of this dimension's scope (`scripts/` is not
  grepped by either recipe command) but noted so nobody "discovers" them later.

---

## Findings

### Dimension 6 — Stub & Placeholder Implementations

**Audit**: `/audit-tech-debt` · **Date**: 2026-09-05 · **HEAD**: `6fba2b0a` (branch `main`)
**Dedup baseline**: `/tmp/audit/tech-debt/issues_all.json` (500 tech-debt issues, 12 open) +
targeted `gh issue list --search` sweeps on `ruleset` / `affliction` / `stealth` /
`orphan` / `skyrim_ruleset`.

---

## Headline: the panic-stub class is genuinely empty

```
$ grep -RInE 'unimplemented!|todo!\(\)|panic!\("not ' crates byroredux
$ echo $?
1        # zero matches
```

Re-confirmed at HEAD. The codebase's stated preference for explicit fallbacks over
panics holds with **zero** exceptions across `crates/` and `byroredux/`. Nothing to
report from the primary discovery recipe.

The second recipe (`// *(stub|TODO: real|placeholder|not yet)`) returns 46 hits, of
which **zero** are undocumented stubs — see the inventory in §Negative Results. The
two findings below came from widening the sweep to the *semantic* stub class
(zero-production-consumer symbols, silently-`None`-returning builders, and stub
justifications whose stated blocker has since shipped).

---

## Findings

### Dimension 7 — Magic Numbers & Hardcoded Constants

**Audit**: `/audit-tech-debt` · **Date**: 2026-09-05 · **Depth**: deep
**Dedup baseline**: `/tmp/audit/tech-debt/issues_all.json` (500 tech-debt issues, 12 open)

## Scope covered

| Target from the dimension spec | Verdict |
|---|---|
| Bare numeric literals vs version codes in `crates/nif/src/blocks/` | **Clean.** No inline `NifVersion(0x…)` construction and no bare `bsver`/version comparison survives in production code — every gate goes through `NifVersion::V*` or `bsver::*`. Closed by #2281 / #2423 / #2424 / #2425 / #1336 / #1630; re-verified, no regression. |
| Vulkan `MAX_*`/`MIN_*` hardcoded inline vs `vk::PhysicalDeviceLimits` | **Clean.** `NON_COHERENT_ATOM_SIZE` (#1759) carries a `cfg(debug_assertions)` device-limit assert; `BINDLESS_CEILING` is `min`-ed against the reported `maxPerStageDescriptorUpdateAfterBindSampledImages`; anisotropy / LOD bias / timestamp period all read from `properties.limits`. Push constants are deliberately left to pipeline-creation validation (documented in `reflect.rs`). |
| Shader `#define` provenance vs `shader_constants_data.rs` | **One structural gap** — see TD7-2026-09-05-02. No *name* collision exists (verified by cross-referencing all 200 generated `#define` names against every shader declaration), but the gate that is supposed to enforce this cannot see three whole categories of declaration. |
| GPU `#[repr(C)]` size literals (`GpuInstance` 160 B, `GpuCamera` 368 B, `GpuMaterial` 432 B) | **Clean.** No inline size literal anywhere in `crates/renderer/src/` or `byroredux/src/` restates these; every consumer uses `size_of::<T>()`. (`water.rs`'s `size_of::<GpuWaterParams>() == 368` is a coincidental value on its own type, correctly asserted.) Doc-comment drift stays Dim 3's (TD3-2026-09-05-01). |
| Frame/ray/cache budgets scattered vs consolidated | **Two findings** — TD7-2026-09-05-01 and -03. `MAX_TOTAL_BONES` / `MAX_MATERIALS` / `MAX_INSTANCES` / `MAX_INDIRECT_DRAWS` are consolidated in `scene_buffer/constants.rs`; `GLASS_RAY_BUDGET` / `MAX_ALPHA_SKIP_LAYERS` / `RT_REFLECTION_MAX_DIST` are consolidated in `shader_constants_data.rs`; the acceleration budgets are consolidated in `acceleration/constants.rs`. The `GpuRayBudget` tier ceilings are the set that never got the same treatment. |
| `fa5c4191` per-pass byte constants (`FROXEL_BYTES_PER_SLOT` / `SVGF_BYTES_PER_PIXEL` / `CAUSTIC_BYTES_PER_PIXEL`) | **Clean.** `screen_scaled_reservation_bytes` derives from all three; each is itself pinned (`froxel_grid_cost_matches_the_memory_budget_doc` even checks the six `Vec<FroxelSlot>` fields still exist). Found no competing hardcoded VRAM figure that should route through them. |
| ESM sub-record sizes hardcoded | **One minor finding** — TD7-2026-09-05-05. Only six exact-equality `len() == N` sites exist workspace-wide in `crates/plugin/src/esm/`; the ubiquitous `>= 4` form-id guards are idiomatic, not debt. WATR's raw byte offsets are individually annotated against xEdit definitions and length-guarded — deliberately **not** filed. |

**Not flagged (out of scope by rule)**: FourCC tags, BSA/BA2/NIF magic numbers, Vulkan format enums, `#ifndef`/`#define` include guards.

---

### Dimension 8 — Dead Code & Backwards-Compat Cruft

**Audit**: `/audit-tech-debt` · **Date**: 2026-09-05 · **Depth**: deep
**Scope**: `crates/` (28 crates), `byroredux/`, `tools/`, workspace `Cargo.toml`s.
**Dedup baseline**: `/tmp/audit/tech-debt/issues_all.json` (500 tech-debt issues, 12 open) + `gh issue view` spot-checks on every referenced number.

---

## Discovery snapshot (re-run 2026-09-05)

| Probe | Result |
|---|---|
| `grep -RInE 'allow\(dead_code\)' crates byroredux` | **43** hits (matches the pre-measured baseline). 5 are `cfg_attr(not(test\|debug_assertions))`-gated, 2 sit in test/example files, 6 are prose mentions inside doc comments — **30 live attributes** on production items |
| `grep -RInE '#\[deprecated' crates byroredux tools` | **0** — the project deletes rather than deprecates. Clean. |
| `// removed:` / `// deleted:` breadcrumbs | **0 code breadcrumbs.** The single `// deleted:` hit (`crates/ui/src/catalog.rs`, in the FO4 callback-catalog block comment) is prose about *game content* not present in the local archives, not a removed-symbol marker. Clean. |
| `_`-prefixed params surviving a refactor | **0.** Every hit is a legitimate `PhantomData` marker, a `#[repr(C)]` `_pad` field, an FFI callback signature (`vk::DebugUtilsMessengerCallback`), or a deliberate stream-advance discard in an ESM/NIF parser (`let _unused = r.u16_or_default();`) |
| `cargo machete` | **3 unused dependencies** across 3 manifests — see TD8-2026-09-05-07 |
| `Cargo.toml` feature flags | 8 features across 5 manifests (`debug-server`, `tracing-tracy`, `dhat-heap` ×2, `save`, `inspect`, `parallel-scheduler`, `recon`). **All 8 have both branches live** — each is either `cfg`-consumed in Rust or gates real `[[example]]`/`required-features` targets. No single-branch flag to remove. |
| `_tmp_*` scratch examples (#3746 regression check) | **0** of 92 example targets. The #3746 fix holds. |
| `cargo check --workspace --all-targets` | 1 warning total (`unused_mut` at `crates/plugin/src/esm/records/grup_walker.rs`, inside a `#[cfg(test)]` closure — excluded per the skill's cfg(test) rule). The compiler-visible dead-code surface is fully covered by the 30 live `allow` attributes above. |

---

## Pre-assigned leads — resolved

### #2266 (Dim 6 hand-off) — **VERIFIED ABSENT FROM MAIN. Do not re-file.**

Dim 6 was right to flag the orphan branch `origin/fix/npc-spawn-dead-code-oblivion-ignore-charal-gmst` (`bbd501a1`, which is **not** an ancestor of `main`), but #2266's specific defect is nonetheless fixed on `main` — by a *different* commit reached through a *different* issue:

```
$ git merge-base --is-ancestor bbd501a1 main   → NOT on main   (orphan branch, #2266/#3084/#3169/#3170)
$ git merge-base --is-ancestor 211a23cc main   → IS on main
$ git show 211a23cc --stat
  211a23cc  2026-09-03  "Fix #3747: delete two dead NPC-spawn compatibility shims"
  -pub fn spawn_npc_entity(
  -pub fn spawn_prebaked_npc_entity(
```

A later audit re-found the same two wrappers as **#3747** (TD8-2026-08-30-02, CLOSED) and that fix *did* land on `main`. Live-tree confirmation: `grep -RIn "spawn_npc_entity\|spawn_prebaked_npc_entity" --include="*.rs" .` returns exactly one hit — a historical comment in `byroredux/src/npc_spawn/tests.rs` ("Extracted out of the old synchronous `spawn_npc_entity` wrapper"). No definition, no `pub use`, no call site.

**Conclusion**: #2266's wrappers are genuinely gone. The orphan branch matters for #3084/#3169/#3170 (Dim 6's scope), not for Dim 8.

### #2268 — referenced, not re-filed
`TD8-003: Dead NIF particle-modifier back-compat shims`. Verified still **OPEN**; `NiPSysGrowFadeModifier` / `NiPSysColorModifier` still present in `crates/nif/src/blocks/particle.rs`. Left to the existing issue.

### #3833 — referenced, not re-filed
`TlasIntegritySnapshot` (declared `crates/renderer/src/vulkan/acceleration/mod.rs`, produced by `AccelerationManager::integrity_snapshot` in `acceleration/tlas.rs`) confirmed still dead — zero workspace consumers outside its own crate. Filed today, still OPEN. My independent scan reproduced it, which is a useful cross-check that the scan works.

---

## Findings

### Dimension 9 — Test Hygiene

**Scope**: `#[ignore]` triage, vacuous/smoke-only assertions, commented-out
assertions, feature-gated tests never enabled in CI, `println!`-without-assert,
`byroredux/tests/golden_frames.rs`, and cross-referencing named regression tests
in sibling audit skills. Also: individual verification of **#3084** (handed over
from Dimension 6's orphan-branch lead) and of the three test groups added today
by `fa5c4191`.

---

## Baseline (re-measured today)

| Metric | Value |
|---|---|
| `#[ignore]` sites, `crates` + `byroredux` (the SKILL.md recipe's scope) | **181** |
| `#[ignore]` sites, whole tracked tree incl. `tools/` | **182** |
| Bare `#[ignore]` (no reason string) | **0** — #3749's convention is holding |
| Total `#[test]` fns discovered | 7 310 |
| `#[ignore]`d tests that *also* soft-skip to a green pass | **≥ 101** (see TD9-…-02) |
| Commented-out assertions (`// assert…!`) | **0** |
| Genuinely assertion-free `#[test]` fns | **0** (19 candidates, all deliberate no-panic/noop — see Verified Clean) |
| Smoke-only (`assert!(x.is_ok())` and nothing else) | **0** (1 candidate, false positive) |
| `println!`-only tests | **0** |

### `#[ignore]` reason categories (all 181 in-scope sites)

Reason text is now machine-readable end-to-end, so this triages without opening
a single function body. **Every one of the 181 is a device- or data-availability
gate.** Not one references an issue number, open or closed — so the severity
table's "guards a fix from a closed CRITICAL/HIGH issue → MEDIUM" trigger fires
**zero** times this cycle.

| Category | Count | Debt? |
|---|---|---|
| Per-game on-disk corpus (`needs <GAME> game data on disk` × 7 titles + combos) | 132 | No |
| BSA/BA2 archive opt-in (`requires <GAME> BSA — opt in with --ignored`) | 13 | No |
| Multi-master / any-installed-game sweeps | 6 | No |
| Audio device + FNV data | 6 | No |
| RT-capable Vulkan device + display/Xvfb | 5 | No |
| Vulkan device + release build (golden frames, upscaler quality) | 3 | No — but see TD9-…-01 |
| Resident-memory-capped whole-master parses (~850 MB – 1.4 GB) | 4 | No (guards the #3843 OOM class) |
| Explicitly-documented manual timing benches | 2 | No — both carry a rationale doc comment and a copy-paste invocation |
| Misc (Workshop Framework archive, two-binary render anchor, quality harness) | 10 | No |

Of the 182, exactly **one** runs in CI: `cornell_rt_oracle` via
`.github/workflows/rt-correctness.yml` (`-- --ignored`). The other 181 run only
when a developer opts in by hand.

---

## Handed-over lead: #3084 — **VERIFIED FIXED ON MAIN, no action**

Dimension 6 flagged that `origin/fix/npc-spawn-dead-code-oblivion-ignore-charal-gmst`
(commit `bbd501a1`, not an ancestor of `main` — re-confirmed) carries the fix for
#3084 among three other closed issues.

**#3084 is nevertheless fixed on `main`, independently.** The guard
`installed_oblivion_creature_assets_resolve_from_their_records`
(`/mnt/data/src/gamebyro-redux/byroredux/src/npc_spawn/tests.rs`) carries
`#[ignore = "needs Oblivion game data on disk; parses the whole master (~1.4 GB resident)"]`
today, landed by `bdc0d84e` ("test: gate the four ungated real-data tests; cut
test-run peak RSS 2828 -> 168 MB") — a different commit from the orphan branch's.
No TD9 finding. The orphan branch's *other* three issues (#2266 / #3170 / #3169)
are dead-code and CHARAL, i.e. Dimensions 6/8 and `/audit-character`, not mine.

The *residual* half of #3084's premise — "a skip can't read as a pass" — is,
however, still live tree-wide, and is filed below as TD9-2026-09-05-02.

## Today's `fa5c4191` tests — **ALL PRESENT, NONE IGNORED**

| Test | File | Status |
|---|---|---|
| `gpu_boundary_instance_stride_matches_gpu_instance` | `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` | present, not ignored |
| `every_static_deferred_push_credits_the_resident_counter` | `crates/renderer/src/vulkan/acceleration/tests/blas_static_tests.rs` | present, not ignored |
| `both_destroy_paths_release_the_resident_counter` | idem | present, not ignored |
| `skinned_entries_are_excluded_from_the_static_counter` | idem | present, not ignored |
| `mid_batch_trigger_uses_resident_bytes_but_the_evict_loop_does_not` | idem | present, not ignored |
| `blas_budget_subtracts_the_resolution_scaled_reservation` | `crates/renderer/src/vulkan/acceleration/tests/predicates_tests.rs` | present, not ignored |

All six run in the default `cargo test --workspace` lane.

---

# Findings

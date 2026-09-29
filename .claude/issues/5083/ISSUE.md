# TD4-2026-09-29-01: Mis-titled fix commits leave about 30 fixed issues OPEN, and today's audits spent passes re-verifying them

**Labels**: medium,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: MEDIUM. Promotion rule: a stale audit baseline that misled audits in the last 90 days.
- **Dimension**: 4 — Audit-Finding Rot (process debt)
- **Location**: git history `ee6d3fb39..HEAD`; the open-issue pool (`/tmp/audit/issues.json`)
- **Status**: NEW
- **Age**: 2026-09-23 → 2026-09-28
- **Effort**: small (sweep) / medium (automation)
- **Description**: several commits fixed issues whose numbers appear nowhere in the message, so GitHub
  never closed them. Under the dedup protocol (`_audit-common.md` step 3: "OPEN → Existing, skip") each
  one reads as live debt, so every audit re-verifies it.
- **Evidence**:
  - `2f8538334` "Enhance audio systems and combat animations" (09-26). I verified these in the diff:
    - #4750: `MAX_OBSCRIPT_VM_NESTING`, `exec_if_chain` / `run_arm_body` depth.
    - #4751: the `ObScriptDiagnostics` doc link.
    - #4749: the `material_translate.rs` SpeedTree placeholder PBR.
    - Plus the #4748 doc half.
  - `546366364` "feat(audio): integrate SoundCache…" (09-27) carries #4739, per the audio report.
  - `63c0aee3b` "Refactor environment variable usage for Skyrim SE data path" (09-27, 43 files) carries
    #4753–#4756 and #4758–#4760, per the tooling report. I verified #4760 myself: the only
    `BYROREDUX_SKYRIM_DATA` reader left is the deliberate `.github/workflows/playable-smoke.yml:45`
    fallback.
  - `88c23887b` "feat(renderer): Enhance volumetrics and groundcover models with GPU timers" carries
    #4866. I verified `model_timer_encloses_all_phases_and_stats_copy` at `groundcover_models.rs:958`.
    Per the renderer report it likely also carries #4783.
  - `0e0d35b96` "Refactor Vulkan Renderer Code and Update Documentation" carries #4868. I verified
    `gpu_timers.rs:461-468`.
  - `b7491072f` "Refactor code structure and remove redundant changes" carried #4620–#4625. Those were
    hand-closed at 12:17Z today.
  - `b9e961eeb`, titled as lighting docs, carried the Starfield units module and +392 LOC of Cornell
    oracle.
  - Still OPEN at HEAD while a sibling report says fixed:
    - audio: #4739, #4740, #4741, #4742, #4744, #4745, #4746
    - speedtree: #4749
    - scripting: #4750, #4751
    - tooling: #4753, #4754, #4755, #4756, #4758, #4759, #4760
    - UI: #3429
    - legacy-compat: #4127
    - renderer: #4778, #4783, #4785, #4862–#4865, #4867, #4868, #4872, #4880. The renderer report
      re-verified only #4868.
  - Signs that audits were misled:
    - The exterior report calls #4866 "the second report recommending closure".
    - The renderer report carried a 13-issue open-but-fixed list forward from 09-27.
    - The audio, scripting, speedtree, tooling, UI and legacy-compat reports each spent a disposition
      pass on these.
- **Impact**:
  - About 30 of the 163 open issues are phantom, and every audit pays to re-verify them.
  - A real regression can hide behind a stale "Existing: #N".
  - The inverse failure (#4768: closed while the debt remained) shows the pool is unreliable in both
    directions.
- **Related**: TD9-2026-09-29-01 (the gate that should have flagged these commits has been dead since
  09-07); #4768; memory notes *Multi-issue Commit Close* and *perf_fix_status_unclosed_issues*.
- **Suggested Fix**:
  - Close each issue listed above that has a sibling-report verification line, citing its commit.
  - Have `/session-close` (or `/audit-publish`) harvest the "Fixed; close" / "stale-open" lines that
    reports already write, and list them for closure.
  - Fix TD9-01 so pushes get annotated again.

**Full close-candidate list** (from the open-but-fixed reconciliation in `docs/audits/AUDIT_REGRESSION_2026-09-29.md`, which re-queried every number with `gh issue view`; all were still OPEN when this issue was filed). "own" marks a spot-check the regression sweep made at HEAD; other rows rest on the named sibling report. This issue does not close any of them; closure is a manual maintainer step.

| Issue | Sev | Fixing commit | Evidence | Verified by |
|---|---|---|---|---|
| #4739 | low | `546366364` | `SoundCache` + `play_oneshot_cached` via `get_or_load` | audio |
| #4740 | low | `546366364` / `2f8538334` | tests use `headless()` | audio |
| #4741 | low | `2f8538334` | `late.rs:236-243` names `camera_follow_system` | audio |
| #4742 | medium | `2f8538334` | player swing removed; `player_swing_without_marked_draugr_does_not_queue_feedback` (`combat_anim.rs:821`) | audio + own (test present) |
| #4744 | low | `2f8538334` | tautological self-match removed | audio |
| #4745 | low | `2f8538334` | all fixtures carry `--sounds-bsa` (successor AUD-2026-09-29-D5) | audio |
| #4746 | low | `2f8538334` | `systems/audio.rs:1,226-229` | audio |
| #4748 | medium | `2f8538334` | resource-determinism disclaimer in `snapshot.rs` + `save-load-roundtrip.md:62-63` | save |
| #4749 | **high** | `2f8538334` | `pbr_classified_at_import` gate (`material_translate.rs:609`), placeholder `false` | speedtree + own |
| #4750 | **high** | `2f8538334` | `MAX_OBSCRIPT_VM_NESTING = 32` (`obscript_vm.rs:53`, checks at `:295`, `:576`) | scripting + own |
| #4751 | low | `2f8538334` | `ObScriptDiagnostics` link fixed | scripting |
| #4753 | medium | `63c0aee3b` | `encode` enforces `MAX_MESSAGE_SIZE`, `MAX_WALK_DEPTH` 64 | tooling |
| #4754 | low | `63c0aee3b` | 10 s timeout + 2 tests (successor TOOL-D1-01) | tooling |
| #4755 | medium | `63c0aee3b` | `requested_gameplay_components_are_registered` (`registration.rs:416`) | tooling + own |
| #4756 | low | `63c0aee3b` | `debug_cli_component_counts_match_the_registry` (`registration.rs:449`) | tooling + own |
| #4758 | medium | `63c0aee3b` | `atomic_write` (successor TOOL-D4-01) | tooling |
| #4759 | medium | `63c0aee3b` | `esm_opens` + `a_present_but_truncated_main_plugin_fails_validation` (`validate.rs:311`) (successor TOOL-D5-01) | tooling + own |
| #4760 | low | `63c0aee3b` | only `playable-smoke.yml:45` repo-var fallback remains | tooling + own |
| #3429 | medium | `e2f99ad55` | same-extent `update_rgba` queued; `record_pending_rgba_uploads` | UI |
| #4127 | low | `8fd66a2b6` | coordinate-system.md matches code | legacy-compat |
| #4778 | low | `e26441c34` | `composite_and_volumetrics_uniforms_match_rust_field_order` (`composite.rs:1620`) | own |
| #4783 | medium | `88c23887b` | `filter_fog_volumes_for_grid` / `fog_volume_within_grid_reach` (`volumetrics.rs:633-665`) | own |
| #4785 | medium | `90c779748` + `e2f99ad55` | sun block gated; local-light rays skipped for zero-scattering froxels (`volumetrics_inject.comp:3040`) — both halves | own |
| #4862 | low | `ab255cfd2` | `shaders_pin_the_mirror_constants` asserts log-space clamp order + `!contains("clamp(val, 0.0, 1.0)")` + THIRD_PARTY_NOTICES pin | own |
| #4863 | low | `ab255cfd2` | `exposure_meter.comp:68` skips NaN/Inf | own |
| #4865 | low | `ab255cfd2` | `static_assert`s in `byro_fsr3.h:82-85` | own |
| #4866 | low | `88c23887b` | `model_timer_encloses_all_phases_and_stats_copy` (`groundcover_models.rs:958`) | exterior + own |
| #4867 | low | `ab255cfd2` | `vizFlags = legacyDebugMode ? dbgFlags : 0u` (`triangle.frag:200`); `legacy_visualization_bits_are_disabled_in_named_debug_modes` | own |
| #4868 | low | `0e0d35b96` | `snapshot_from_bits_with_valid_bits` (`gpu_timers.rs:471`) | renderer + own |
| #4872 | low | `0e0d35b96` | memory-budget.md "0, 14, or 28 MiB per slot" (`:184`), noise pair 294,912 B (`:491`) | own |
| #4880 | **high** | `e26441c34` | `record_dds_upload` destroys image on allocate failure (`texture.rs:468`), frees on view failure (`:529-535`), cleanup before recording (`:501`) | own |

**Close candidates: 31.** Keep open: **#4864**. Its docs are aligned, but its Suggested Fix needs an `upscaler_quality` A/B or a RenderDoc mask capture to choose the reactive-mask policy, and that was never recorded. For #4745, #4754, #4758 and #4759, close with a pointer to the successor finding each fix introduced (AUD-2026-09-29-D5, TOOL-D1-01, TOOL-D4-01, TOOL-D5-01).

**Validated at HEAD 9fcfdc3fc**: `gh issue view` on all 32 listed numbers: every one is still OPEN; spot-checked the fix sites `MAX_OBSCRIPT_VM_NESTING` (`obscript_vm.rs:53`), `model_timer_encloses_all_phases_and_stats_copy` (`groundcover_models.rs:958`), `snapshot_from_bits_with_valid_bits` (`gpu_timers.rs:471`), `requested_gameplay_components_are_registered` / `debug_cli_component_counts_match_the_registry` (`registration.rs:416/449`), `filter_fog_volumes_for_grid` (`volumetrics.rs:634`), `player_swing_without_marked_draugr_does_not_queue_feedback` (`combat_anim.rs:821`); the carrying commits (`2f8538334`, `63c0aee3b`, `546366364`, `88c23887b`, `0e0d35b96`) name none of these issue numbers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

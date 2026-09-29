# TD1-2026-09-29-08: Per-frame driver functions keep regrowing after #4342; only `draw_frame` has a budget (regression of #4342)

**Labels**: low,renderer,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

**Regression of #4342** (#4342 is CLOSED; filed as a new issue rather than reopening it).

- **Severity**: LOW · **Dimension**: 1 · **Status**: Regression of #4342 (partial) · **Effort**: medium
- **Location**:
  - `byroredux/src/app_events.rs:611` `about_to_wait`: 969 lines. It was 822 when #4342 was filed, and
    +104 in this window.
  - `crates/renderer/src/vulkan/context/geometry_pass.rs:21` `record_geometry_pass`: 763 (+131).
  - `byroredux/src/app_frame.rs:117` `render_one_frame`: 656. #4342 extracted it down to 591.
  - `byroredux/src/asset_provider/material/merge.rs:469` `merge_bgsm_arm`: 669 (+111).
- **Evidence**:
  - `about_to_wait` growth came from `6d05c2bc0` (#4992 boot guard), `4790227ce` (#4208 pin),
    `a070baaad` (view toggle), `0182fc5e8` (gear import) and `766e1746e` (dialogue). Each added a hook
    inline.
  - Functions that newly crossed 200 lines: `prepare_mesh_upload_range` 259, `npc_combat_ai_system_inner`
    233, `refresh_scene_actor_bindings` 228, `trigger_detection_system` 227, `spawn_object_lod_quad` 221,
    `pre_parse_cell` 216, `fill_scratch_telemetry` 216.
- **Suggested Fix**:
  - Extract the per-feature hooks (dialogue, gear import, view toggle) as `App` methods *inside*
    `app_events.rs`, since its `include_str!` scans at :1746/:1860/:1901 must keep matching.
  - Add budget pins like `draw_frame`'s for `about_to_wait` and `render_one_frame`.

**Validated at HEAD 9fcfdc3fc**: brace-counted function lengths at HEAD: `about_to_wait` (`app_events.rs:611`) 969, `record_geometry_pass` (`geometry_pass.rs:21`) 763, `render_one_frame` (`app_frame.rs:117`) 656, `merge_bgsm_arm` (`merge.rs:469`) 669; no budget pin exists for any of them (only `draw_frame`'s).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

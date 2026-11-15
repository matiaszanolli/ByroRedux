# #6988: TD8-2026-11-15-04: Twenty-two `pub` items are referenced nowhere in the workspace; ten were reported on 2026-10-12 and #5745 closed without them, twelve were never listed (regression of #5745)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6988
- **Labels**: low,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 8
- **Age**: ten items from the TD8-2026-10-12-03 list (oldest `f03a7711e`, 2026-04-12); twelve not on that list: `end_journey` `4b71155ca` (2026-11-13), `body_perk_count` `18063a780` (2026-11-08), `request_linear_capture` `75b4024df` (2026-11-02), `run_console_command` `4b99e2393` (2026-10-18); older ones `refracts` `f62d4bd54`, `try_add_to_with_access` `b362e8812`, `resolve_lstring` `f43f84c71`, `enqueue_dds_with_clamp` `7c6c156bc` (2026-04/05)
- **Effort**: trivial
- **Labels**: tech-debt
- **Status**: Regression of #5745 (CLOSED; its Suggested Fix covered these ten and they are still present); #6189 and #6397 are the same pattern and show the sibling deletions keep getting partially done
- **Description**: a whole-repo identifier count (`.rs` under `crates`, `byroredux`, `tools`; comments excluded; tests and examples included) is exactly 1 (the definition) for each item below. Still-live from the 10-12 list: `b_from_base` (`crates/core/src/character/derived.rs:310`), `allocated_count` / `free_list_depth` (`crates/core/src/ecs/resources/skin_slot_pool.rs:365,371`), `requiring_restart` (`crates/core/src/settings/mod.rs:141`), `set_override_str` / `clear_override` / `set_screen` (`crates/menuxml/src/menu.rs:280,287,374`), `count_by_kind` (`crates/spt/src/scene.rs:178`), `is_avm2` (`crates/ui/src/profile.rs:59`), `OBLIVION_WORLDSPACE` (`crates/plugin/src/esm/cell/worldspace.rs:26`). New or never listed:
  - `VulkanContext::request_linear_capture` and `linear_capture_pending` (`crates/renderer/src/vulkan/context/linear_capture.rs:99,119`): the CLI uses `request_linear_capture_at` (`byroredux/src/linear_capture.rs:154,192`), yet the CLI module's own doc (`:8`) names `request_linear_capture` as the renderer entry point. Doc and code disagree.
  - `WaterKind::refracts` (`crates/core/src/ecs/components/water.rs:139`): `docs/engine/watal.md:1094` says the canonical type lets translate "disable rays for opaque waterfalls (`WaterKind::refracts`)". The shader instead branches on the `WATER_WATERFALL` / `WATER_LAVA` kind constants, so the CPU helper restates a decision made elsewhere and nothing calls it.
  - `Scheduler::try_add_to_with_access` (`crates/core/src/ecs/scheduler.rs:546`), `resolve_lstring` (`crates/plugin/src/esm/records/common.rs:129`; only the `_in` variant is called), `enqueue_dds_with_clamp` (`crates/renderer/src/texture_registry/upload.rs:182`), `color_subresource_mips` (`crates/renderer/src/vulkan/descriptors.rs:231`), `DEFAULT_BLOOM_INTENSITY` (`crates/renderer/src/vulkan/bloom.rs:148`), `IDENTITY_BONE_SLOT` (`crates/renderer/src/vulkan/scene_buffer/constants.rs:73`, kept alive by one intra-doc link), `run_console_command` (`crates/scripting/src/obscript_quests.rs:1072`; the `obscript.cmd` command it documents calls `_with_text`), `end_journey` (`crates/scripting/src/created_references.rs:318`), `body_perk_count` (`crates/scripting/src/perk_entry_points.rs:534`).
- **Impact**: dead surface, a few of which document themselves as the entry point (`request_linear_capture`, `refracts`). `end_journey` and `body_perk_count` are under two weeks old and read as speculative accessors.
- **Related**: #5745, #6189, #6397, TD8-2026-10-12-03.
- **Suggested Fix**: delete the twenty-two (the crate-public ones are the skill's "note, do not delete" class, so route `is_avm2`, `set_screen`, `count_by_kind`, `requiring_restart` and the `skin_slot_pool` pair as one decision; the rest are plain deletions). Fix the `byroredux/src/linear_capture.rs:8` doc to name `request_linear_capture_at`. Either delete `refracts` or have the water translate call it so the shader constant and the CPU helper cannot diverge. Not counted here: the bink2 `decode_frame` / `decode_frame_with_maps` / `parse_line_map` / `LineMaps` / `Frame::quadrant_mv` set, which is labelled "Diagnostics (P32)" and serves the research harness that lives in another repository.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix

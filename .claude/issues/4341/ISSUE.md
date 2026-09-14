# #4341 — TD1-005: `setup_scene` is 1056 lines with nesting depth 10, and has regrown past its 2026-08 size

**Labels**: low, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4341

- **Severity**: LOW · **Dimension**: 1
- **Location**: `byroredux/src/scene.rs:720-1775` · **Status**: NEW · **Age**: 760 on 08-20 → 996 on 09-04 → 1056; latest jump `7019eb84b` (09-08) · **Effort**: medium · **Kind**: tech-debt
- **Finding**: One body does harness decode → content-source chain (Cornell / ESM / loose NIF, `:774-1099`) → `--kf` → demo primitives → camera spawn → player-mode selection + ground probe (`:1367-1593`) → SSBO/descriptor finalize → `--menu` launch. The phases share six mutable locals.
- **Suggested Fix**: Extract in place, in order: `load_scene_content` → `SceneContent`, `start_cli_animation`, `spawn_initial_camera`, `select_and_spawn_player` (keep #2375's probe-before-mode ordering inside it), `finalize_scene_gpu_buffers`, `launch_archive_menu`.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix

# #6949: TD1-2026-11-15-03: Vulkan constructors `volumetrics/init.rs::new_inner` (614 code lines, largest function in the repo) and `composite.rs::new_inner` (434, +46) have no budget (regression of #2256)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6949
- **Labels**: low,renderer,vulkan,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Age**: volumetrics 594 at 670f81917 to 614 now (d3e3acc7b, 626197ae5, 2f3e16891, 684bc9161 and 0e55ec32b added the combustion and local-fire stages). composite 388 to 434 (0e55ec32b, local-fire bindings).
- **Effort**: medium (construction only, no recording)
- **Labels**: `tech-debt`
- **Location**: `crates/renderer/src/vulkan/volumetrics/init.rs:167-880` and `crates/renderer/src/vulkan/composite.rs:582-1190`
- **Status**: NEW for composite. For volumetrics, **regression of #2256** (TD1-078, "volumetrics.rs crossed 2000 LOC, plus a 556-LOC constructor"; the file was split but the constructor was left at 556 span lines and is now 714 span / 614 code).
- **Description**: `volumetrics::new_inner` builds, in one body: the six per-frame froxel volumes (`for i in 0..MAX_FRAMES_IN_FLIGHT`, `:238`), three samplers (`:373-401`), the params buffers (`:418`), the inject layout / pool / sets / pipeline (`:528-690`), the integrate params / layout / pool / pipeline (`:695-800`), and the combustion stage after it. All of it is guarded by the `partial` + `try_or_cleanup!` rollback pattern. The stages are already commented and separate; they only lack function boundaries. `composite::new_inner` has the same shape.
- **Impact**: This is the only >500 code-line function left (the other 494-line driver is pinned). The three stage-1 volumetrics commits of 10-31 to 11-07 each grew it.
- **Suggested Fix**: Follow the precedent of `context/init_stages.rs` (#6143): one `build_<stage>(&mut partial, ..) -> Result<()>` per stage (`create_volumes`, `create_samplers`, `build_inject_stage`, `build_integrate_stage`, `build_combustion_stage`), with `new_inner` as the sequencing driver, each stage keeping its `try_or_cleanup!` rollback. Pin the driver in `function_size_pin_tests.rs`. Construction only; no barrier or order change. Verify the composite constructor's rollback shape before applying the same split to it.
- **Related**: #2256, #6143, #6371 (compute-pass teardown tracker).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix

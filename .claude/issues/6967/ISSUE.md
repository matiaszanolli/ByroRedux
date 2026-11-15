# #6967: TD3-2026-11-15-02: `shader-pipeline.md` says the depth buffer is standard (0 = near, clear 1.0, LESS_OR_EQUAL); reversed-Z has shipped

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6967
- **Labels**: low,renderer,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW (promotion candidate: it is the "authoritative" renderer reference; I could not show it misled an audit, `audit-renderer/SKILL.md:78` already states reversed-Z correctly). **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: 09940503b (2026-11-07, #6642 DLOD-DEPTH-002) flipped the mapping; the commit updated `exterior-readiness-plan.md` and ROADMAP only.
- **Evidence**: `crates/core/src/ecs/components/camera.rs:192` (`ACTIVE_DEPTH_MAPPING = DepthMapping::Reversed`), `crates/renderer/src/vulkan/pipeline.rs:174` (`DEPTH_CLEAR_VALUE = ACTIVE_DEPTH_MAPPING.clear_value()`, 0.0 under reversed), test `the_shipped_depth_mapping_is_reversed` at `camera.rs:1378`. Stale:
  - `docs/engine/shader-pipeline.md:490` ("Standard depth (0.0 = near, 1.0 = far), `LESS_OR_EQUAL`, clear = 1.0")
  - `docs/engine/coordinate-system.md:320-321` ("shipped: Conventional")
  - `docs/prd/distant-lod-completion.md:158` ("shipping `DepthMapping::Conventional` ... pinned by `the_shipped_depth_mapping_is_conventional`") and `:509` (the test no longer exists)
- **Fix**: shader-pipeline.md:490 -> reversed (1.0 = near, clear 0.0, the compare op the pipeline derives for it); coordinate-system.md:321 -> "shipped: Reversed"; PRD row 158 -> Reversed, `the_shipped_depth_mapping_is_reversed`. Name `ACTIVE_DEPTH_MAPPING` and let the number follow it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

# Batch: #2044, #2045, #2046, #2051

## #2044 — TD4-003: audit-scripting SKILL.md Phase-1 baseline fully stale
- Severity: HIGH · Labels: documentation, high, tech-debt
- Location: `.claude/commands/audit-scripting/SKILL.md:103,146-152`
- Fix: delete "no prior audit exists" sentence + hardcoded preloaded-issue list
  (#1663-1668, #1316, all CLOSED); replace with standard `_audit-common.md`
  "read most recent docs/audits/AUDIT_SCRIPTING_*.md, diff direction" instruction.
- SIBLING check: sweep other audit-* SKILL.md files for the same staleness class.

## #2045 — TD7-101: triangle.frag hand-writes INST_RENDER_LAYER_SHIFT/_MASK
- Severity: HIGH · Labels: bug, renderer, high, tech-debt
- Location: `crates/renderer/shaders/triangle.frag:80-81,402` vs.
  `crates/renderer/src/vulkan/scene_buffer/constants.rs:216-217`
- Fix: add INSTANCE_RENDER_LAYER_SHIFT/_MASK to `shader_constants_data.rs`,
  let build.rs emit them into the generated header, delete the 2 hand-written
  shader consts, add `instance_render_layer_bits_match_scene_buffer_consts`
  lockstep test mirroring existing INSTANCE_FLAG_* guards.
- Domain: renderer (byroredux-renderer)

## #2046 — TD2-103: finish_partial_import missing game-aware BSXFlags bit-5 fix (LIVE BUG)
- Severity: MEDIUM (live bug) · Labels: bug, import-pipeline, medium, legacy-compat, tech-debt
- Location: `byroredux/src/cell_loader/partial.rs:54-59` vs.
  `byroredux/src/cell_loader/references/import.rs:69-100`
- Fix: partial.rs's async exterior-streaming drain path unconditionally treats
  BSXFlags bit 5 as "editor marker" and skips import; on Skyrim+/FO4/FO76/
  Starfield bit 5 means MultiBoundNode, not editor marker (fix commit
  6feac029 in references/import.rs, game-era gated via bsver). PartialNifImport
  struct has no bsver field. Suggested: extract shared
  build_cached_nif_import(scene, bsx, bsver, ...) helper; thread bsver through.
- Domain: binary (byroredux, cell_loader) — import-pipeline
- SIBLING check: any other streaming/async NIF-import path re-implementing gate.
- TESTS: FO4 exterior-streaming regression test, BSXFlags=0xA2 architecture not dropped.

## #2051 — TD4-005: audit-speedtree SKILL.md stale latest-report + stale finding-status
- Severity: MEDIUM · Labels: documentation, medium, tech-debt
- Location: `.claude/commands/audit-speedtree/SKILL.md:97-103`
- Fix: replace hardcoded "_2026-07-02.md latest" + "SPT-NEW-01/06/07 still
  unfiled" (actually filed as #1820/#1821/#1822, #1820+#1821 closed, only
  #1822 open) with standard "read most recent, diff direction" instruction.

## Domain classification
- #2044 → docs/commands only, no crate test target
- #2045 → `byroredux-renderer`
- #2046 → `byroredux` (binary crate, cell_loader)
- #2051 → docs/commands only, no crate test target

# Issue #3465: NIFAL-2026-08-27-05: doc rot — the texture vocabulary is 22 named roles, not 18, and the two-phase boundary now has three translate_material callers, not "both spawn sites"

**Labels**: low, nifal, documentation, doc-rot
**Filed**: 2026-08-27 via /audit-publish

**Severity**: LOW
**Dimension**: Shader-flags / texture sets (Dim 8) + Material (Dim 1)
**Tier Violated**: — (documentation)
**Game Affected**: all
**Location**: `docs/engine/nifal.md:489`, `.claude/commands/audit-nifal/SKILL.md:240`, `docs/engine/nifal.md:587`, `byroredux/src/material_translate.rs:43-46`, and the stale spawn-site line numbers in `byroredux/src/render/static_meshes.rs:415-418`
**Source**: `docs/audits/AUDIT_NIFAL_2026-08-27.md` — NIFAL-2026-08-27-05

## Description

Three separate staleness items, all introduced by the 08-24→08-27 window's work:

1. `nifal.md:489` — *"Its 18 named roles plus four ordered decal layers"* — and
   the identical claim with an explicit 18-item list in `SKILL.md:240`.
   `MaterialTextureSet` now has **22** named roles
   (`crates/nif/src/import/types.rs:309-343`): the list is missing
   `lighting_mask`, `back_lighting`, `glass_roughness_scratch` and
   `glass_dirt_overlay`. `values()` and its parity test agree at 22 + 4 = 26
   (`types.rs:381-407`, `canonical_iteration_covers_every_role_once`), so only the
   prose is wrong — but the SKILL text is the checklist an auditor diffs
   `values()` against, which is the one hand-written role walk the compiler does
   not protect.
2. `nifal.md:587` and `material_translate.rs:43-46` both say the Phase-2 resolvers
   *"run at **both** spawn sites"*. There are now three production
   `translate_material` callers; `cell_loader/placement_lod.rs:527` is the third
   and calls neither resolver. That is harmless today — it attaches no
   `MaterialTextureHandles`, so both resolvers would early-return — but "both" no
   longer identifies the set, and the reason the third is exempt is recorded
   nowhere.
3. `static_meshes.rs:415-418` still cites *"both spawn sites
   (cell_loader/spawn.rs:841 and scene/nif_loader.rs:793)"* as the audit evidence
   for the deleted render-side glass heuristic. The cell-path `Material`
   construction moved to `cell_loader/spawn/mesh_instance.rs` under `#2057`;
   neither line number resolves.

## Evidence

`grep -c` on the struct gives 22 `pub <role>: T` fields plus `decals: [T; 4]`;
`grep -n "translate_material(" byroredux/src` gives three production call sites
(`scene/nif_loader.rs:915`, `cell_loader/placement_lod.rs:527`, plus the cell
spawn site; `cornell.rs:1994` is the synthetic harness);
`sed -n '841p' byroredux/src/cell_loader/spawn.rs` is unrelated code.

## Impact

Documentation only, but item 1 degrades the very checklist the Dimension-8
role-walk audit depends on, and item 3 is the kind of stale citation `#1114`'s
path-reference convention exists to prevent.

## Related

- `#1114` (path/symbol reference convention)
- `#2330` (the two-phase boundary)
- `#2057` (the split that moved the spawn site)

## Suggested Fix

Update the role count and list in both files; reword the Phase-2 sentence to
"every `translate_material` caller that attaches `MaterialTextureHandles`" and
note why `placement_lod` does not; refresh the two line numbers in
`static_meshes.rs` (or drop them for symbol names, per the convention).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other role-count / spawn-site claims in `nifal.md`, `SKILL.md`, and module docs)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix (a source-scan assertion that the documented role list matches `MaterialTextureSet` would make item 1 self-maintaining)

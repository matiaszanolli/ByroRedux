# SPT-NEW-05: Foliage texture-path substring collisions in the PBR keyword classifier mis-tag vanilla trees as wood/glass

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1819
**Source report**: docs/audits/AUDIT_SPEEDTREE_2026-07-02.md
**Labels**: high, import-pipeline, bug

- **Severity**: HIGH
- **Dimension**: NIFAL Material Translation
- **Location**: `crates/core/src/ecs/components/material.rs:449-489` (`classify_pbr_keyword` — `"glass"/"crystal"/"ice"/"gem"` arm at :483, `"wood"/"plank"/…` arm at :489), reached via `byroredux/src/material_translate.rs` (`translate_material` → `Material::resolve_pbr`); `crates/spt/src/import/mod.rs:328-334` (`placeholder_billboard_mesh` ships `metalness_override: None, roughness_override: None`)
- **Status**: NEW (raised in `AUDIT_SPEEDTREE_2026-07-01.md` as SPT-NEW-05, never filed; re-verified in `AUDIT_SPEEDTREE_2026-07-02.md` — the `"ice"`/`"gem"`/`"wood"` substring arms are present unchanged at `material.rs:483`/`:489`)

**Description**: The SpeedTree placeholder billboard is the only production content type that reaches `resolve_pbr`'s keyword-classifier "sentinel backstop" arm in practice — every real NIF mesh extractor classifies at import time and sets `metalness_override: Some(...)`, so the classifier never fires for NIF content. The placeholder mesh literal sets both overrides to `None`, so `translate_material` seeds `Material.metalness = NaN` and `resolve_pbr` runs `classify_pbr_keyword` against the resolved leaf texture path (TREE.ICON or the `.spt` tag-4003 fallback). That classifier uses plain case-insensitive substring matching with no word-boundary check and no foliage bucket. Two real vanilla Oblivion tree textures collide:
- `ShrubBoxwoodLeaves*.dds` (`shrubms14boxwood.spt`) contains `"wood"` → WOOD (`roughness 0.7`) instead of the matte foliage default (`0.85`).
- `ShrubGenericElderberryLeaves*.dds` (`ShrubGenericElderberry{FA,SU}.spt`) contains `"ic"+"e"` across the `generIC` / `Elderberry` word seam (`…generICE lderberry…`) → GLASS (`roughness 0.1`). 0.1 crosses the RT-reflection gate (`< 0.6` triggers ray-traced reflections in `triangle.frag`), so the leaf billboard would render mirror-smooth — a visible "glass leaf" artifact.

**Evidence**: Prior audit ran the compiled `classify_pbr_keyword` against real extracted texture names (Boxwood → `roughness 0.7`; Elderberry → `roughness 0.1`; WhiteOak → correct `0.85`). This audit re-confirmed the colliding substring arms are present and unchanged in current `material.rs`, and that `import/mod.rs:333-334` still emits `metalness_override: None, roughness_override: None`.

**Impact**: Visual-only (no crash / no GPU hazard); metalness stays `0.0` (never promoted metallic). But roughness is wrong for at least two vanilla Oblivion species and the Elderberry case visibly crosses the RT-reflection threshold on foliage. Per `_audit-severity.md`, "wrong/divergent `Material` out of NIFAL `translate_material`" is an unconditional HIGH floor — no per-draw fallback masks a wrong resolved `f32` once it lands on the canonical `Material`. Blast radius: any `.spt`-backed tree whose leaf path contains an architecture/weapon/cloth/skin keyword substring; the full FO3/FNV corpus was not exhaustively scanned, so this is likely not the only collision.

**Related**: SPT-D4-04 / #1001, SPT-D5-02 / #1002 (the sizing-precedence findings — this is the PBR-classification analogue). Distinct from #1346/#1365 (classifier doc-framing, not correctness). Cross-cuts `/audit-nifal` (single-boundary is respected; this is a classifier-taxonomy gap on the backstop arm, not a bypass).

**Suggested Fix**: (a) Have `placeholder_billboard_mesh` set explicit `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)` so the SpeedTree importer classifies-at-import like every NIF path — narrow, parity-preserving, no shared-classifier change. OR (b) add word-boundary / foliage-bucket matching to `classify_pbr_keyword` if the backstop arm is meant to stay reachable for future non-NIF content. (a) is lower-risk.

## Completeness Checks
- [ ] **SIBLING**: The full FO3/FNV `.spt`-backed foliage texture corpus (not just Boxwood/Elderberry) is scanned for other keyword-substring collisions before closing
- [ ] **CANONICAL-BOUNDARY**: The fix stays at the parser→`Material` boundary (`placeholder_billboard_mesh` import-time override, or `classify_pbr_keyword` itself) — never pushed into the shader/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins Boxwood → foliage-default roughness and Elderberry → foliage-default roughness (not WOOD/GLASS)

# RT-3: every Starfield skinned mesh has 100% unresolved bones — all SF actors and apparel render in bind pose

**Issue**: #3549
**Labels**: bug, nif-parser, high, nif, game:starfield
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-3. Measured on a live Starfield `citycydoniamainlevel` run.

## Description

**401 of 485** skinned-mesh log lines report `UNRESOLVED`, across **73 unique meshes**, each with *every* bone unresolved:

```
Skinned mesh 'Naked_M:0': 41 bones (41 UNRESOLVED — names: ["Bone0", "Bone1", ...]), root=Some("ExportScene")
```

Same for `Hands_3rd_M`, `Outfit_NewAtlantis_FashionableSuit_01_*`, `Outfit_Miner_Jumpsuit_*`, `Outfit_Baseball_Cap_*`.

## Contrast — measured, not assumed

The identical counter is **0** on every other game: fnv 0/217, skyrim_se 0/133, fo4 0/299, oblivion 0/4, fo3 0/7. This is Starfield-specific, not a general skinning defect.

## Evidence

The log line is emitted at `byroredux/src/scene/nif_loader.rs:1250`. The bone names Starfield NIFs carry are generic placeholders (`Bone0`...`BoneN`) under `root="ExportScene"`, and nothing matches them to the skeleton's node names.

`crates/nif/src/import/mesh/skin.rs` resolves bones **by string name** throughout (`resolve_node_name`, lines 145 / 263 / 284 / 293 / 405 / 593 / 607). Note that `skin.rs:405` and `:593` *already* fall back to synthesizing the literal name `Bone{i}` when `resolve_node_name` returns `None` — which is exactly the placeholder shape observed in the logs, so the parser is manufacturing the names it then fails to match.

## Impact

Every Starfield NPC and every piece of Starfield apparel renders in **bind pose**. This is the whole SF character-rendering surface.

## Suggested Fix

Starfield skin data evidently indexes bones **positionally** rather than by name. Resolve SF `BSSkin::Instance` bone references by **index into the skeleton's bone array** instead of by string, gated on `bsver >= SF_FORM_ID`, in `crates/nif/src/import/mesh/skin.rs`. Verify against the placeholder-name fallback at `:405`/`:593` so the two do not mask each other.

## Completeness Checks
- [ ] **SIBLING**: All bone-resolution sites in `crates/nif/src/import/mesh/skin.rs` (`build_imported_bones`, the BSSkin path, and the two `Bone{i}` fallbacks) handled consistently
- [ ] **CANONICAL-BOUNDARY**: The per-game index-vs-name decision stays at the NIF parser boundary — never re-derived at render time or in the skin palette pass
- [ ] **TESTS**: A regression test pins index-based SF bone resolution (per-block corpus baseline or a synthetic BSSkin fixture)

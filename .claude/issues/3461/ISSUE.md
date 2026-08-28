# #3461 — NIF-2026-08-27-D3-01: `BSDistantObjectExtraData` has no dispatch arm — 112,716 `NiUnknown` blocks on FO76 distant-LOD content

Source: `docs/audits/AUDIT_NIF_2026-08-27.md`
Filed: 2026-08-27 via `/audit-publish`
Labels: medium, nif-parser, nif, bug, game:fo76

---

Audit: `docs/audits/AUDIT_NIF_2026-08-27.md` — Dimension 3 (Block Dispatch Coverage). Severity **MEDIUM**. Game: **Fallout 76** (`bsver` 152–167).

## Location
`crates/nif/src/blocks/mod.rs` (`parse_block_inner` — no arm exists; `grep -rn BSDistantObjectExtraData crates/nif/src` returns **zero** hits, re-verified at publish time).

## Description
nif.xml documents this block completely at line 8460 —
`<niobject name="BSDistantObjectExtraData" inherit="NiExtraData" module="BSMain" versions="#F76#">` with a single field
`<field name="Distant Object Flags" type="uint" />`. There is no parser and no dispatch arm, so every instance falls through to the `NiUnknown` placeholder and its flags word is discarded.

## Evidence
Measured live over three FO76 archives (`--release`):
- `SeventySix - GeneratedMeshes01.ba2` — 20,245 files, **41,435** `NiUnknown`, 100% `BSDistantObjectExtraData` (e.g. `meshes\terrain\appalachia\objects\appalachia.4.-42.-53.bto`)
- `SeventySix - GeneratedMeshes02.ba2` + `SeventySix - 10UpdateMain.ba2` — **71,281** more, same single type
- `SeventySix - Meshes.ba2` (the one gated archive) — 58,469 files, **0** `NiUnknown`

The block lives on `.bto` distant-LOD terrain meshes, which ship in the `GeneratedMeshes*` / `*UpdateMain` archives the corpus gate never opens.

## Impact
FO76 has a `block_sizes` table, so this does not cascade — the outer reconciliation absorbs it and `scene.truncated` stays false. The loss is the per-object distant-LOD flags word on 112,716 blocks, the same class of loss #942 fixed for `BSDistantObjectInstancedNode` ("ghost foliage"). Blast radius is confined to FO76 distant terrain.

## Related
Closed #942 (sibling block, same subsystem); the corpus-gate blind spot that hid this is filed as its own issue (NIF-2026-08-27-D3-02).

## Suggested Fix
One arm — `"BSDistantObjectExtraData" => Ok(Box::new(NiExtraData-base + read_u32_le()))`, mirroring the other single-field extra-data parsers, plus the matching `per_block_baselines` regeneration. nif.xml settles the layout; no research needed.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other extra-data block parsers, the `per_block_baselines` / `block_coverage_baselines` regen)
- [ ] **TESTS**: A regression test pins this specific fix

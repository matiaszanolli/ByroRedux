# RT-9: Oblivion gained one genuine base-color texture miss — `facegen\ears\human\earshuman.dds`

**Issue**: #3555
**Labels**: bug, import-pipeline, low, game:oblivion
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-9.

## Description

Oblivion / `ICMarketDistrictTheGildedCarafe` — the "cleanest path" cell, baselined at **0** base-color texture misses — now has **1**.

Missing path:
```
facegen\ears\human\earshuman.dds  [slot=base_color]
```

Its `_n` sibling (`facegen\ears\human\earshuman_n.dds`) also misses, so both an authored diffuse and its derived normal are unresolved for FaceGen ear geometry.

## Why this one is real

This survives the RT-4 correction. Re-scored against the *pre-#3349* metric surface (base-color slot only) it is still 1 vs a baseline of 0 — it is a genuine miss, not an artifact of the metric widening. It is the **only** genuine `tex_missing` delta across all five captured games.

## Suggested Fix

Check whether `facegen\` needs the same archive-layout resolution the `.spt` `trees\` case needed (#3528): a top-level prefix that the mesh-path normalizer is either prepending `textures\` to when it should not, or failing to prepend when it should. Note RT-12 documents an adjacent normalization defect (two cache keys for one asset under two path spellings) on FO4 — same family.

## Completeness Checks
- [ ] **SIBLING**: Every other top-level archive prefix (`trees\`, `facegen\`, `lodsettings\`, ...) checked against the same normalizer, not just `facegen\`
- [ ] **TESTS**: A resolver test pins `facegen\`-prefixed paths through the real archive layout
- [ ] **BASELINE**: Hold `oblivion-ICMarketDistrictTheGildedCarafe.tsv`'s `tex_missing` row stale until this is fixed

# RT-12: FO4 counts one asset twice under two path spellings — two cache keys for one texture

**Issue**: #3558
**Labels**: bug, import-pipeline, low, game:fo4
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-12.

## Description

FO4 / `InstituteBioScience` — `tex.missing` lists the **same asset twice** under two path spellings:

```
90x  textures\setdressing\wallconsoles\wallconsole01_sm_d_n.dds
18x  setdressing/wallconsoles/wallconsole01_sm_d_n.dds
```

One with a `textures\` prefix and backslashes, one without and forward-slashed. Same pattern on `effects/colorwhiteutility_n.dds` and `actors/character/hair/hair{short,long}01grayscale_d_n.dds`.

## Impact

Two effects, the second worse than the first:

1. Inflates `tex_missing_unique_paths` — a purely cosmetic metric problem.
2. **More importantly**: it implies two separate **cache keys** for one texture. For any path that *does* resolve, that is a second archive lookup and potentially a second resident copy in VRAM.

## Suggested Fix

Normalize separator and the `textures\` prefix **before the cache key is taken**, not just before the archive lookup. The lookup path evidently already normalizes (both spellings reach the archive); the registry key does not.

Related shape: RT-9 (oblivion `facegen\` prefix miss) is the same normalizer family from the other direction.

## Completeness Checks
- [ ] **SIBLING**: Every path-keyed cache in the asset provider checked (texture registry, mesh cache, material path resolution), not just the texture one
- [ ] **TESTS**: A regression test pins that `textures\a\b.dds` and `a/b.dds` produce one cache entry

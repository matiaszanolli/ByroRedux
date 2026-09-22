# NIFAL-D8-2026-09-21b-01: BGSM merge's NIF-first texture precedence is an unsourced runtime claim — keeps a disagreeing/dead NIF path on 1.1% of FO4 normal-mapped shapes

**Issue**: #4636
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIFAL_2026-09-21b.md)

**Severity**: LOW
**Dimension**: Shader flags / texture roles (merge boundary)
**Tier Violated**: no-fabrication
**Game Affected**: FO4
**Location**: `byroredux/src/asset_provider/material/merge.rs:139-149` (`fill`) and `:151-158` (doc, introduced by `1b032b6bb` / #493, no citation).

## Description
`fill` fills a texture slot only when `None`, so a non-empty NIF-authored path always wins over the resolved BGSM/BGEM chain. The doc claims this "matches Bethesda's runtime behaviour" with no citation. 56,560 FO4 shapes carry both a NIF normal slot and a leaf-BGSM `normal_texture`; 637 (1.1%) name different files. A hard sub-case: 3 shapes bind a NIF-slot path (`.../femalebody_msn.dds`) absent from every vanilla FO4 BA2, while their BGSM names a path that does resolve — the NIF-first rule leaves them with no normal map at all.

## Suggested Fix
Source the precedence rule (CK / NifSkope / nifly behaviour) and cite it in `merge.rs:151-158`. Independent of that: a spawn-time fallback to the BGSM chain's path when the NIF slot fails to resolve closes the dead-path sub-case regardless of which precedence is correct.

## Source
docs/audits/AUDIT_NIFAL_2026-09-21b.md (NIFAL-D8-2026-09-21b-01)

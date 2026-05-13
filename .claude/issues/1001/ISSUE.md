# #1001 — SPT-D4-04: Default 256×512 placeholder size mis-tuned for Oblivion

- **Severity**: LOW
- **Domain**: legacy-compat
- **Audit**: `docs/audits/AUDIT_SPEEDTREE_2026-05-13.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1001

## TL;DR
Default `(256, 512)` is FNV/FO3-scaled. Oblivion TREE records often lack OBND (and always lack BNAM), so they fall back to the default — vanilla Oblivion conifers should be ~512×1024.

## Fix
Preferred: compute the default from MODB (bound radius) when present — Oblivion ships MODB more reliably than OBND. Sphere radius → bbox half-extent conversion.

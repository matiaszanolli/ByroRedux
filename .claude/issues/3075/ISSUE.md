# NIFAL-D9-2026-08-16-01: nifal.md understates the material boundary set and collision shape count

**Issue**: #3075
**Severity**: LOW
**Labels**: `low,nif-parser,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_NIFAL_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_NIFAL_2026-08-16.md` (Dimension 9 — spec accuracy).

**Location**: `docs/engine/nifal.md`:317, :605, and the "Materials" section at :66-99

## Description

`nifal.md` — **the layer spec** — understates both the material boundary set and the collision shape count.

## Evidence

Re-verified 2026-08-17:
```
:317  All 13 parsed `bhk*Shape` variants now translate. Remaining collision *non*-leaks are
:605  (BhkMultiSphereShape, BhkConvexListShape); all 13 parsed shape variants now
```

The "13 variants" figure appears twice and does not match the live dispatch set. The Materials section at :66-99 likewise names fewer boundary sites than exist — `merge_external_material`, `pack_imported_material_flags` and `Material::resolve_pbr` are all part of the boundary.

## Impact

`nifal.md` is named in `_audit-common.md`'s reference-doc table as authoritative for the canonical translation layer, with the instruction to prefer it over re-deriving from source.

An understated boundary set is the dangerous direction: a contributor who consults the spec to check "is this a NIFAL boundary?" gets a shorter list than reality and may add per-game logic somewhere the spec does not mention — which is exactly the violation #3073 documents.

## Suggested Fix

Refresh both figures against the live `resolve_shape` dispatch and the actual boundary set. Since the shape count has now drifted, consider stating it as a pointer to the dispatch function rather than a number.

## Related

- #3073 (NIFAL-D1-01 — a live boundary violation at a site the spec does not name)
- #2971, #3028, #3047 — the same named-authoritative-doc rot class elsewhere in this sweep

## Completeness Checks
- [ ] **COUNT-OR-POINTER**: The shape count is corrected, or replaced by a pointer that cannot drift
- [ ] **BOUNDARY-SET**: All live boundary sites named, including `merge_external_material` and `pack_imported_material_flags`
- [ ] **SIBLING**: `docs/engine/physal.md` / `exal.md` checked for the same drift
- [ ] **PATH-GATE**: `.claude/commands/_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3075 --json state` when live state is needed.*

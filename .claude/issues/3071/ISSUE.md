# SKY-2026-08-16-D2-02: 1,564 properties author a slot-7 back-lighting map with no canonical role

**Issue**: #3071
**Severity**: MEDIUM
**Labels**: `medium,nif-parser,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_SKYRIM_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SKYRIM_2026-08-16.md` (Dimension 2 — shader flags / slot routing).

**Location**: `crates/nif/src/import/material/slot_role.rs`:148-155 · role enum at :36-54

## Description

**1,564 vanilla properties author a slot-7 back-lighting map with no canonical role**, and nothing tracks the loss.

The slot-7 arm handles `MULTI_LAYER_PARALLAX` (returns `None`, documented as back-lighting) and the model-space-normal specular case, but the back-lighting data itself has no `TextureRole` variant to land in:

```rust
// Slot 7 is the alternate specular on model-space-normal materials,
// independent of shader type (#2742) — except on type 11, where it is a
// back-lighting map with no canonical role.
7 => match (shader_type, model_space_normals) {
    (bs_lighting::MULTI_LAYER_PARALLAX, _) => None,
    (_, true) => Some(TextureRole::Specular),
    (_, false) => None,
},
```

## Impact

1,564 authored back-lighting maps are dropped at import. The comment acknowledges the gap — *"a back-lighting map with no canonical role"* — but the drop is silent: no counter, no warning, nothing that would surface it in `tex.missing` or any diagnostic.

Distinct from the FO4 slot-7 finding (#2998), which is about the MSN gate excluding *specular* on FO4. This is about the back-lighting role not existing at all.

## Suggested Fix

Add a `TextureRole::BackLighting` variant routed to a `MaterialTextureSet` slot, or — if back-lighting is deliberately deferred — count the drops so the deferral is visible rather than invisible.

The precedent is #2997/#2999, where the canonical destinations (`greyscale_lut`, `wrinkle`) already existed and only the enum variant was missing.

## Related

- #2998 (FO4-D5-07 — the same slot, different gate and different game)
- #3068 (SKY-D2-01 — the same file's slot-2 mis-roling)
- #2742 (the MSN specular rule this arm implements)

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Any new role is routed at the parser→`Material` boundary
- [ ] **NOT-SILENT**: If the drop stays, it is counted or logged rather than invisible
- [ ] **SIBLING**: Resolved consistently with #2998, which touches the same arm
- [ ] **TESTS**: A regression test asserts a type-11 slot-7 binding reaches its role or is counted

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3071 --json state` when live state is needed.*

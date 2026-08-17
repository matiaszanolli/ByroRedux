# NIF-D4-2026-08-16-03: FO76 slot 6 dropped — the only slot-6 arm is a Skyrim shader type FO76 cannot produce

**Issue**: #3085
**Severity**: MEDIUM
**Labels**: `medium,nif-parser,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_NIF_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_NIF_2026-08-16.md` (Dimension 4 — slot routing).

**Location**: `crates/nif/src/import/material/slot_role.rs` (the slot-6 arm)

## Description

FO76 populates texture-set **slot 6 on 1,622 properties**. `slot_to_role`'s only slot-6 arm keys on a **Skyrim shader type that FO76's enum cannot produce**, so all of them are dropped.

## Evidence

```rust
// crates/nif/src/import/material/slot_role.rs (re-verified 2026-08-17)
6 => match shader_type {
    bs_lighting::MULTI_LAYER_PARALLAX => Some(TextureRole::InnerLayer),
    _ => None,
},
```

`bs_lighting::MULTI_LAYER_PARALLAX` is `0x0100_0000` in the Skyrim numbering. FO76 uses the `BSShaderType155` numbering (`#BS_F76# == 155`), which is a different enum — so the match arm is unreachable on FO76 content and every slot-6 binding falls to `_ => None`.

The arm's own comment documents its evidence base as Skyrim-only: *"slot 6 is non-empty on 607/607 type-11 properties in `Skyrim - Meshes0.bsa`"*.

## Impact

1,622 authored FO76 slot-6 textures are silently dropped at import — no warning, no counter.

This is the **fourth instance** of one pattern in this file: a slot arm whose evidence is a Skyrim archive measurement, applied to a game whose shader-type enum or authoring convention differs. #2997, #2998 and #2999 are the FO4 instances; this is the FO76 one.

## Suggested Fix

Make the slot-6 arm game-aware at the same seam proposed for #2997 — normalise the shader type at the call site in `dedicated_shader.rs`, which has `bsver` in scope, rather than widening the shared table's signature.

Measure what FO76 slot 6 actually carries before choosing a role — do not assume it mirrors Skyrim's inner-layer semantics.

## Related

- **#2997, #2998, #2999 (the FO4 instances of this same pattern — fix at one seam)**
- #3057 (SF-D8-01 — the Starfield non-coverage of the same table)
- #2579 (SKY-D2-01, 2026-06 — FO76 `BSShaderType155` numbering leaking into a Skyrim-numbered consumer; the same enum mismatch, different consumer)

## Completeness Checks
- [ ] **ONE-SEAM**: Fixed at the same game-awareness seam as #2997, not with a fifth ad-hoc branch
- [ ] **NO-GUESSING**: The FO76 slot-6 role is measured from shipped content, not assumed to match Skyrim
- [ ] **CANONICAL-BOUNDARY**: Game-awareness lives at the parser→`Material` boundary
- [ ] **NOT-SILENT**: Unrouted slots are counted, so the next such gap is visible
- [ ] **TESTS**: A regression test asserts an FO76 slot-6 binding reaches a role

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3085 --json state` when live state is needed.*

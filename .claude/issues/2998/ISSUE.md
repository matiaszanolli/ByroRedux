# FO4-D5-07: Slot 7 carries the specular map on FO4 but is gated on model_space_normals

**Issue**: #2998
**Severity**: MEDIUM
**Dimension**: 5 — shader flags / slot routing
**Labels**: `medium,nif-parser,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FO4_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FO4_2026-08-16.md` (Dimension 5 — shader flags / slot routing).

**Location**: `crates/nif/src/import/material/slot_role.rs`:151-155

## Description

The slot-7 arm returns `Some(TextureRole::Specular)` **only when `model_space_normals` is true**, on the strength of #2742's measurement that "390/390 slot-7-bearing SkinTint properties in `Skyrim - Meshes0.bsa` are MSN".

The inverse holds on FO4: slot 7 is the single most-populated optional slot in the game, and essentially **nothing** sets the MSN flag.

## Evidence

```
slot-7 non-empty bindings   Meshes.ba2  63,314   MeshesExtra.ba2  694,315
                            combined   757,629   (dominated by _s suffix)
MODEL_SPACE_NORMALS set     Meshes.ba2      61   MeshesExtra.ba2        0
                            combined        61   of 810,489 properties
```

`fo4_slsf1::MODEL_SPACE_NORMALS == skyrim_slsf1::MODEL_SPACE_NORMALS == 0x1000`, pinned by an equality assertion at `crates/nif/src/shader_flags.rs`:426-427 — **so this is not a wrong-bit artefact of the measurement.**

Note the internal tension it exposes: FaceTint slot 1 carries 1,079 `_msn.dds` files while only 61 properties archive-wide set the flag — on FO4 the flag is not the signal the filename convention implies.

Re-verified 2026-08-17: `7 => match (shader_type, model_space_normals) { (MULTI_LAYER_PARALLAX, _) => None, (_, true) => Some(Specular), (_, false) => None }`.

## Impact

`(_, false) => None` drops the specular map on **~757,568 of 757,629 bindings**.

Partially masked: 681,525 + 77,766 properties also name a `.bgsm`/`.bgem`, and `merge_external_material` supplies the specular / smoothness side from the external material. The **~50k properties with no external material lose specular outright**, and the drop is unconditional — it happens whether or not a BGSM will cover for it.

Severity held at MEDIUM only because of that mitigation; the routing itself is wrong on FO4.

## Suggested Fix

Make the slot-7 arm game-aware alongside slot 3 (#2997) — on FO4, slot 7 is specular irrespective of MSN. Keep the `(_, true)` Skyrim behaviour.

## Related

- #2742 (CLOSED — the Skyrim measurement being generalised)
- #2997 (FO4-D5-06 — same game-awareness seam; fix together)

## Completeness Checks
- [ ] **SIBLING**: Fixed at the same seam as #2997 rather than with a second ad-hoc branch
- [ ] **CANONICAL-BOUNDARY**: Game-awareness at the parser→`Material` boundary, not in the shader
- [ ] **NO-DOUBLE-WRITE**: Verify the NIF-side specular does not now conflict with `merge_external_material`'s BGSM value — decide which wins
- [ ] **TESTS**: A regression test asserts an FO4 non-MSN slot-7 binding lands in the specular role

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2998 --json state` when live state is needed.*

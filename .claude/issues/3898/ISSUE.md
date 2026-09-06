# #3898: FO4-2026-09-05-D2-01: the BGSM palette-enable bit is dropped whenever the NIF already filled the greyscale_lut role — second gate on #3897

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3898 --json state`).*

---

**Audit**: `docs/audits/AUDIT_FO4_2026-09-05.md` (suite preset `texture-roles-deep`)
**Severity**: MEDIUM · **Dimension**: 2 (BGSM/BGEM)

## Description

The second, independent gate shutting off FO4's greyscale→palette remap (companion to the HIGH, #3897).

`merge_external_material`'s #2108 enable-capture is gated on the role slot still being empty:

```rust
// byroredux/src/asset_provider/material.rs:1480
if material.textures.greyscale_lut.is_none() && !bgsm.greyscale_texture.is_empty() {
    material.bgsm_greyscale_lut_enabled = bgsm.base.grayscale_to_palette_color;
    material.bgsm_greyscale_lut_color   = bgsm.base.grayscale_to_palette_color;
}
```

#2997 made `textures.greyscale_lut` populated from NIF slot 3 on FO4, so on exactly the meshes that matter this condition is now permanently false — and the BGSM's own authored `grayscale_to_palette_color` bit is discarded along with it.

The gate's reasoning is sound for the *texture* (a closer BGSM already won the slot, so don't overwrite it) but it also skips the *flag*, which is a separate piece of information the winning source never supplied.

## Evidence

Gate at `byroredux/src/asset_provider/material.rs:1480-1485`. The BGEM sibling at `:1801` has the same `greyscale_lut.is_none()` shape.

**Measured this audit**: 239 of the 246 BGSMs referenced by slot-3-carrying meshes author the `grayscale_to_palette_color` bit; **28,467** properties are affected.

## Impact

Even with the HIGH (#3897) fixed, FO4 assets whose palette enable comes from the BGSM rather than the NIF shader flags stay unremapped. The two gates are independent: closing either alone produces no visible change, which makes this easy to misdiagnose as "the fix didn't work".

## Suggested Fix

Separate the two concerns at the merge: keep the texture-slot precedence as-is, but capture the enable bit whenever the BGSM authors one and the material has not already had it set by a closer source. The existing tests at `byroredux/src/asset_provider/tests/bgsm_merge.rs:1113-1137` already pin the forwarding contract and should keep passing.

## Completeness Checks
- [ ] **SIBLING**: The BGEM arm at `material.rs:1801` carries the same `is_none()` shape — check whether it has the same defect
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins that a NIF-supplied slot 3 plus a BGSM-authored enable bit reaches the GPU with the flag set
- [ ] **TESTS**: A regression test pins this specific fix

## Related
- #3897 (the first gate — must be fixed together to see any change)
- #2108, #2643, #2997

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.

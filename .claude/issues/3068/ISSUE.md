# SKY-2026-08-16-D2-01: slot 2 bound as glow map without reading SLSF2_Glow_Map — 4,922 properties mis-roled

**Issue**: #3068
**Severity**: HIGH
**Labels**: `high,nif-parser,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_SKYRIM_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SKYRIM_2026-08-16.md` (Dimension 2 — shader flags / slot routing).

**Location**: `crates/nif/src/import/material/slot_role.rs`:105-110 · **false premise** at `crates/nif/src/shader_flags.rs`:201 and :219-221

## Description

`slot_to_role` binds `BSShaderTextureSet` slot 2 as the **glow map unconditionally** for every non-tint shader type, without reading `SLSF2_Glow_Map`:

```rust
2 => match shader_type {
    bs_lighting::FACE_TINT | bs_lighting::SKIN_TINT | bs_lighting::HAIR_TINT => Some(TextureRole::Tint),
    _ => Some(TextureRole::Emissive),
},
```

The justification is recorded in `shader_flags.rs` and is **false**:

> `:201` — "Bit 6 is `Glow_Map` on FO4 (Skyrim doesn't have an SLSF2 glow bit)"
> `:219-221` — "Bit 6 — `Glow_Map`. FO4-specific — Skyrim's glow signal is the texture-set slot-2 presence, not a flag bit."

## Evidence

**nif.xml — the authoritative format spec — defines `Glow_Map` at bit 6 for Skyrim's `ShaderFlags2`**, verified 2026-08-17:

```
/mnt/data/src/reference/nifxml/nif.xml:6415
  <option bit="6" name="Glow_Map">Use Glow Map in the third texture slot.</option>
:6487
  <option bit="6" name="Glow_Map" />
:6313
  2: Glow(SLSF2_Glow_Map)/Skin/Hair/Rim light(SLSF2_Rim_Lighting)
```

Line 6313 is explicit that slot 2 is multiplexed — glow **or** skin/hair **or** rim lighting — and the flag is what disambiguates.

Corpus survey over `Skyrim - Meshes0.bsa` (67,105 pre-FO4 `BSLightingShaderProperty` blocks):

```
non-tint properties with slot 2 authored:                    6,253
  SLSF2_Glow_Map SET   (slot 2 genuinely a glow map):        1,331
  SLSF2_Glow_Map CLEAR (bound as glow map anyway):           4,922
    ... SLSF2_Soft_Lighting set (subsurface mask):           3,561
    ... SLSF2_Rim_Lighting  set (rim mask):                    155
  mis-roled AND emissive_color non-black  → LIVE:              383
```

## Impact

**4,922 vanilla Skyrim properties bind a non-glow texture into the emissive role.** 3,561 of them are subsurface-scattering masks and 155 are rim-lighting masks — both get rendered as self-illumination.

383 are **live today**: they are mis-roled *and* carry a non-black `emissive_color`, so the wrong texture is actively modulating emission on screen.

The false premise in `shader_flags.rs` is the reason no prior audit questioned this — it reads as a researched, deliberate decision.

## Suggested Fix

Gate the slot-2 → `Emissive` routing on `SLSF2_Glow_Map` for Skyrim, and route the `Soft_Lighting` / `Rim_Lighting` cases to their own roles (or explicitly to `None` with a comment, if no canonical role exists yet).

**Correct the two `shader_flags.rs` comments** — they are the load-bearing false claim, and leaving them invites the same conclusion again.

## Related

- #3071 (SKY-D2-02 — slot 7 back-lighting, same file, same "no canonical role" gap)
- #2997/#2998/#2999 (the FO4 slot-routing findings in this same table)

## Completeness Checks
- [ ] **NO-GUESSING**: The flag semantics come from nif.xml, not inference
- [ ] **FALSE-PREMISE**: `shader_flags.rs`:201 and :219-221 corrected so the claim cannot be re-derived
- [ ] **CANONICAL-BOUNDARY**: Routing decided at the parser→`Material` boundary, never in the shader
- [ ] **SIBLING**: The `Soft_Lighting` / `Rim_Lighting` cases get a role or an explicit documented `None`
- [ ] **TESTS**: A regression test asserts a `Glow_Map`-clear slot-2 property does not land in the emissive role

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3068 --json state` when live state is needed.*

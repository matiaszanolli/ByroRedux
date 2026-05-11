# NIF-D3-NEW-01: until=X semantic mismatch with niftools/nifly (exclusive vs inclusive)

**Severity**: HIGH
**Source audit**: `docs/audits/AUDIT_NIF_2026-05-10.md` (Dim 3)

## Game Affected

Pre-Bethesda content (Civ4 Colonial Fleet, IndustryGiant 2, Morrowind-era mods, v10.0.1.2 BSStreamHeader files). **Shipping Bethesda titles unaffected** — every gate sits at versions older than 20.0.0.5. This is the most plausible root cause of the long-tracked NiSourceTexture/NiTexturingProperty 1-byte shortfall on legacy content.

## Locations

- `crates/nif/src/blocks/texture.rs:53-58` (`NiSourceTexture::Use Internal` `until="10.0.1.3"`)
- `crates/nif/src/blocks/properties.rs:451` (TexDesc PS2 L/K `until="10.4.0.1"`)
- ~10 other call sites carrying `// see #765 sweep` comments

## Why it's a bug

niftools' own token table (`/mnt/data/src/reference/nifxml/nif.xml:6-9`) defines `#NI_BS_LTE_FO3#` with operator `#LTE#` and description "All NI + BS *until* Fallout 3" — `until` is colloquial for **inclusive** `<=`. nifly mirrors this:

```cpp
// /mnt/data/src/reference/nifly/src/Shaders.cpp:25
if (fileVersion <= NiFileVersion::V10_0_1_2)
// /mnt/data/src/reference/nifly/src/Objects.cpp:217
if (fileVersion <= NiFileVersion::V10_0_1_3)
```

ByroRedux's #765 sweep chose `<` (exclusive). On `v=10.0.1.3` exactly, `NiSourceTexture::Use Internal` is skipped (1 byte under-read). On `v=10.4.0.1`, `TexDesc::PS2 L/K` is skipped (4 bytes under-read).

## Fix

Audit every site with `// see #765 sweep` or `// exclusive` comment. Flip `< NifVersion(0xN)` → `< NifVersion(N+1)`. Document the inclusive semantic at the top of `version.rs`. Pair with a regression test using a sample v10.0.1.3 NiSourceTexture.

Bundles NIF-D1-NEW-01 (NiStencilProperty Flags — siblings need flipping) and NIF-D1-NEW-02 (target_color unconditional read).

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: All `// see #765 sweep` sites swept; NiStencilProperty siblings (NiMaterialProperty/NiFogProperty) flipped to match
- [ ] **DROP**: N/A
- [ ] **TESTS**: Regression test for v10.0.1.3 NiSourceTexture Use Internal byte
- [ ] **DOCS**: Top-of-`version.rs` doc comment documents inclusive `until=` semantic per niftools/nifly

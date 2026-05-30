# Investigation — #1293 Starfield XCLL tail

## Premise was WRONG (the key finding)
#1293 asked to "decode the 16-byte SF tail (108-92)", stating the existing arm
"decodes through 92 bytes correctly (Skyrim+ ambient cube)". Verified against the
**authoritative xEdit SF1 source** (`Core/wbDefinitionsSF1.pas` on the `xedit-4.1.5p`
branch, fetched via the GitHub blobs API) AND real `Starfield.esm`: **that premise is
false.** Starfield's XCLL shares only bytes 0-39 with Skyrim, then diverges at offset
40 into a *volumetric height-fog model* — there is no Skyrim ambient cube / specular /
fresnel. So ByroRedux was **misdecoding ~52 bytes (40-91)** of every Starfield cell,
not merely dropping a 16-byte tail.

### Authoritative layout (xEdit SF1, byte-verified)
```
 0 Ambient / 4 Directional / 8 Fog Color Near (RGBA each)   ┐ shared with Skyrim
12 Fog Near / 16 Fog Far / 20,24 Dir Rotation XY,Z          ┘ (0-27)
28 Gravity Scale      (Skyrim has "Directional Fade" here)
32 Fog Clip / 36 Fog Power                                   (shared offsets)
40 Fog Color Far  44 Fog Max  48 Light Fade Begin  52 Light Fade End
56 Unknown(RGBA) 60 Near Height Mid 64 Near Height Range
68 Fog Color High Near 72 Fog Color High Far
76 High Density Scale 80 Fog Near Scale 84 Fog Far Scale
88 Fog High Near Scale 92 Fog High Far Scale
96 Far Height Mid 100 Far Height Range
104 Interior Type (u8 enum 0-4) + 105 Unused(3)              = 108
```
**Skyrim (TES5)** at 40-91 is instead `Ambient Colors` (6×RGBA cube + specular +
fresnel) + Fog Color Far + Fog Max + Light Fade + Inherits-flags — confirmed by
fetching `wbDefinitionsTES5.pas`. So the divergence is genuinely Starfield-specific.

### Empirical confirmation (Starfield.esm, all 11 985 108-byte cells)
A temporary probe aggregated the distinct tail prefixes: byte 104 ∈ {0,1,2,3,4}
(exactly the 5-value `Interior Type` enum) and bytes 105-107 = CK heap-fill garbage
(0xAB / 0xEF / 0x00) = xEdit's `Unused(3)`. The 3 f32 at 92/96/100 vary per-cell
(authored), proving the tail is real data, not padding.

## Fix (full Starfield decode path — user-approved scope)
- `cell/mod.rs`: new `StarfieldLighting` struct (13 SF-only fields + `interior_type`)
  and `CellLighting.starfield: Option<StarfieldLighting>`.
- `cell/walkers.rs`: dedicated `game == Starfield && len == 108` branch in the
  `b"XCLL"` arm decoding the SF layout; the 40-55 fog-far-colour / max / light-fade
  map onto the existing base fields, `directional_ambient`/`specular_*`/`fresnel_power`
  and `directional_fade` are `None` (SF has none). `continue` skips the Skyrim path.
  Stale "decodes through 92 / tail undecoded" docs corrected.
- 4 mechanical `starfield: None` additions at the other `CellLighting` construction
  sites (`load.rs` LGTM fallback + 3 test literals in `components.rs` + 1 in
  `lgtm_fallback_tests.rs`) — required fan-out of adding the field.
- New regression test `parse_cell_starfield_xcll_decodes_volumetric_height_fog_tail`.

Renderer-side consumer wiring of the new SF fog fields is **separate scope** (per the
issue) — but this fix already STOPS the active misdecode: SF cells no longer get a
garbage `directional_ambient` cube / specular / fresnel (now correctly `None`).

## Completeness checks
- **SIBLING (FO76)**: the branch is gated `game == Starfield`, so FO76 never takes it;
  a hypothetical 108-byte FO76 XCLL falls to the Skyrim path + trips the existing
  size-sanity warn. The TES5-vs-SF1 comparison confirms the layout is SF-only (not a
  backported Creation Engine extension). Safe.
- **TESTS**: synthetic 108-byte SF XCLL → asserts every SF field + that the Skyrim
  fields stay `None`. Plugin lib 447 pass; byroredux bins 388 pass.
- **UNSAFE / DROP / LOCK_ORDER / FFI**: N/A.

## Verification
Starfield.esm: 11 985 interior cells parse cleanly, zero XCLL warns/panics. Workspace
builds clean.

## Sources
xEdit SF1 `Core/wbDefinitionsSF1.pas` + `wbDefinitionsTES5.pas` (branch `xedit-4.1.5p`),
real `Starfield.esm`. Parent #1291 (size pin). OpenMW ESM4 has no Starfield support;
Gibbed.Starfield has no XCLL field layout — xEdit is the authority.

# #3506: FO3-2026-08-27-D3-01: the documented WRLD.DATA flag bit map is the TES5 list shifted one bit down — wrong for FO3's TES4 layout, and wrong for TES5 too

**Labels**: medium, esm-plugin, documentation, doc-rot, game:fo3, legacy-compat
**Audit**: `docs/audits/AUDIT_FO3_2026-08-27.md`

---

Source: `docs/audits/AUDIT_FO3_2026-08-27.md` — finding `FO3-2026-08-27-D3-01` (MEDIUM, Dimension 3 — ESM record coverage / wire-layout contract).

## Location
- `crates/plugin/src/esm/cell/mod.rs` — `WorldspaceRecord` wire-layout docblock, the `DATA` bullet (~L911-913)
- read at `crates/plugin/src/esm/cell/wrld.rs` — `b"DATA" if !sub.data.is_empty() => { record.flags = sub.data[0]; }` (~L188-190)

## Description
The `WorldspaceRecord` wire-layout docblock states:

```rust
/// - `DATA` — worldspace flags byte (u8): 0x01 small-world, 0x02
///   can't fast travel, 0x04 no LOD water, 0x08 no landscape, 0x10
///   no sky, 0x20 fixed dimensions, 0x40 no grass.
```

The same docblock names OpenMW `components/esm4/loadwrld.cpp` as its reference. That reference (`/mnt/data/src/reference/openmw/components/esm4/loadwrld.hpp:46-56`) gives two *different* columns:

```cpp
enum WorldFlags     // TES4                 TES5
{
    WLD_Small          = 0x01,  // Small World          Small World
    WLD_NoFastTravel   = 0x02,  // Can't Fast Travel    Can't Fast Travel
    WLD_Oblivion       = 0x04,  // Oblivion worldspace
    WLD_NoLODWater     = 0x08,  //                      No LOD Water
    WLD_NoLandscpe     = 0x10,  // No LOD Water         No Landscape
    WLD_NoSky          = 0x20,  //                      No Sky
    wLD_FixedDimension = 0x40,  //                      Fixed Dimensions
    WLD_NoGrass        = 0x80   //                      No Grass
};
```

The committed comment is the **TES5** name list assigned to bit positions one step *below* their TES5 values. Every flag from `No LOD Water` upward is therefore documented at the wrong bit for both families, and for FO3 — which is TES4-layout — the errors are semantic, not just positional: FO3's `0x04` is *Oblivion worldspace*, and its `0x10` is *No LOD Water*, not "no sky".

## Evidence
The mislabelling is not academic on real FO3 data. Live WRLD census of `Fallout3.esm` (32 worldspaces):

```
    MegatonWorld  flags=0x51   →  documented: "no grass + no sky + small world"
                              →  TES4 (OpenMW): bit6 + No LOD Water + Small World
     CitadelWorld flags=0x11 · MonumentWorld 0x11 · WashMonTop 0x11 · tLandscape 0x11
     ParadiseFalls 0x13 · TranquilityLane 0x13 · StatesmanRoofWorld 0x03 · Wasteland 0x00
```

Reading MegatonWorld as "no sky" is plainly wrong — Megaton's exterior renders a sky. Under the TES4 column the same byte reads as "No LOD Water", which matches a crater settlement with no distant water plane.

## Impact
Zero today — `WorldspaceRecord::flags` is parsed and **no engine code reads it** (grep over `byroredux/src` and `crates/`: only the parser writes it, plus test assertions). That is precisely why this is worth filing now: the first consumer to land — an EXAL sky/landscape/LOD-water gate is the obvious candidate, and `0x10` is the bit such a gate would reach for — will build on a wrong bit map and silently suppress the wrong subsystem on 9 of FO3's 32 worldspaces. The cost of fixing a wire-layout comment now is a one-line edit; the cost after a consumer exists is a per-game rendering bug that looks like a renderer defect.

## Related
Same class as #3101 / #1887 — a per-game premise recorded in a comment that real data falsifies. Owner-audit note: the *mechanism* (`b"DATA"` dispatch) is `/audit-esm`'s; the per-game *semantics* is the FO3 audit's remit, which is why it was filed there.

## Suggested Fix
Replace the single list with the two columns OpenMW carries, keyed by `EsmVariant`/`GameKind`, and say explicitly that no consumer exists yet so the next one reaches for the right column.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game flag-byte docblocks in `crates/plugin/src/esm/cell/`)
- [ ] **TESTS**: A regression test pins this specific fix (the existing `crates/plugin/src/esm/cell/tests/wrld.rs` flag assertions should name the TES4 semantics)

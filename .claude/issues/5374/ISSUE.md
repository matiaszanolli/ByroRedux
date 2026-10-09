# #5374: EXT-D5-2026-10-08-01: Oblivion child worldspaces inherit no default water — `default_water_for_worldspace` models inheritance only through FO3+ `PNAM`, so Bravil's canals, Leyawiin's river, the Imperial City lake shore and New Sheoth ren...

**Labels**: high,terrain-exterior,water,bug,game:oblivion
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5374

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-08.md` — `EXT-D5-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: HIGH. A wrong canonical value comes out of an EXAL boundary producer, and the skill's EXAL row sets HIGH. The missing water is in flagship content: the Imperial City plus two major cities.
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `byroredux/src/env_translate.rs:209-238` (`default_water_for_worldspace`). Line 217: `inherit_up_chain(.., pnam::INHERIT_WATER, ..)`. Lines 220-229: the Oblivion arm, whose comment says "Oblivion authors no PNAM either, so the chain walk is a no-op there".
  - `crates/plugin/src/esm/cell/wrld.rs:103-112` (`parent_flags` stays 0 when PNAM is absent).
  - Consumer: `byroredux/src/cell_loader/exterior.rs:1972-1976` and `:71-81` (`resolved_exterior_water_height`).
- **Status**: NEW. It is pre-existing: #2735 recorded "Oblivion is unaffected (it authors no PNAM)", and that is the premise this finding disproves. Searches for "BravilWorld", "Oblivion child worldspace", "Oblivion parent worldspace water", "city worldspace water" and "Oblivion PNAM" found no tracker.
- **Tier Violated**: n/a. The translation itself is wrong: no PNAM is read as "inherit nothing", but in Oblivion it means "inherit everything".
- **Game Affected**: Oblivion (including the Shivering Isles worldspaces in Oblivion.esm)
- **Description**:
  - A child worldspace with no own `NAM2` resolves `(None, None)`. Every cell without `XCLW` then gets no water plane.
  - Oblivion child worldspaces never author `NAM2`, `CNAM` or any PNAM.
  - The data has the same "absent ⟺ inherited" shape that #2735 used to establish PNAM semantics in the later games.
- **Evidence** (Oblivion.esm census):
  - **54/54** root WRLDs author `NAM2`; **0/30** child WRLDs (`WNAM` set) do. 27/54 roots author `CNAM`, 0/30 children do.
  - LAND cells with a VHGT minimum below Z=0 (Tamriel's sea level) and no `XCLW`:

    | Worldspace | Cells below Z=0, no XCLW / LAND cells |
    |---|---|
    | BravilWorld | 33/47 |
    | LeyawiinWorld | 22/38 |
    | ICTalosPlazaDistrict | 29 |
    | ICElvenGardensDistrict | 21 |
    | ICTempleDistrict | 15 |
    | ICImperialPrisonDistrict | 14 |
    | ICArboretumDistrict | 13 |
    | ICMarketDistrict, ICImperialPalace, ICTheArcaneUniversity | 12 each |
    | AnvilCastleCourtyardWorld | 10 |
    | ICArenaDistrict | 9 |
    | AnvilWorld | 7 |
    | SETheFringe (New Sheoth) | 11 |
    | SENSBliss | 4 |
    | SENSCrucible | 3 |
    | SEVitharnWorld | 2 |

  - Bruma, Cheydinhal, Skingrad, Chorrol and Kvatch have 0 such cells and are unaffected.
- **Impact**: Visible missing water in Bravil, Leyawiin, Anvil, all Imperial City districts and New Sheoth. The near field shows exposed canal and lake beds. Tamriel itself is correct (its own `NAM2` gives Z=0).
- **Related**: #2735 (closed; premise disproved); #1305; EXT-D1-2026-10-08-01 (same root cause, climate half); #5335 (Oblivion distant water).
- **Suggested Fix**:
  - At the parse/translate boundary, give Oblivion children (`parent_worldspace` set, no PNAM) an inherit-all parent flag word. One table-shaped rule (e.g. "pre-FO3: child ⟹ inherit LAND|WATER|CLIMATE") lets `inherit_up_chain` resolve `NAM2` from Tamriel / SEWorld.
  - Pin it with an Oblivion-shaped fixture (child without NAM2 → parent's form, Z=0) and correct the #2735 comment at `env_translate.rs:222-224`.

## Completeness Checks
- [ ] **SIBLING**: Same inheritance gap checked for every other `inherit_up_chain` consumer (LAND/CLIMATE bits, `resolve_worldspace_climate`, LOD water) on Oblivion children
- [ ] **TESTS**: A regression test pins this specific fix

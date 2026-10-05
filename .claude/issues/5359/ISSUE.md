# #5359: SKY-D3-2026-10-05-02: Skyrim's per-NPC skin (`NPC_.WNAM`) is never decoded — 442 vanilla actors, including Alduin, every Draugr variant skin and the Falmer variants, wear their race's default skin instead

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5359
- **Labels**: medium,bug,game:skyrim,legacy-compat,esm-plugin,character
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-10-05.md` (SKY-D3-2026-10-05-02)

_From `docs/audits/AUDIT_SKYRIM_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: MEDIUM
- **Dimension**: 3 — NPC equip + FaceGen (Skyrim body source)
- **Location**:
  - `crates/plugin/src/esm/records/actor/mod.rs:1948`: the only skin decode, on `RACE` (`RaceRecord.default_skin`). The `NPC_` parser has no `WNAM` arm; `grep 'b"WNAM"' crates/plugin/src` finds no NPC hit.
  - `byroredux/src/npc_spawn.rs:1280`: `if let Some(skin_fid) = race.default_skin`. The race skin is the only intrinsic body layer.
- **Status**: NEW.
  - `gh` searches across all states for "worn armor", "WNAM skin", "Alduin", "SkinDraugrMale" and "default skin NPC" find no match.
  - #2093 (CLOSED) added only the RACE `WNAM` decode.
  - Sibling reports dated 2026-09-2x / 2026-10-0x do not mention it.
- **Description**:
  - **What WNAM is.** A Skyrim NPC_ can override its race's body. In the CK it is the **Skin** field on the Traits tab, and the "Use Traits" template flag governs inheriting it. Source: `ck-uesp-wiki` *Creating a Stable*, Step 04: "below the Race drop down box, click on the Skin drop down box … the horse wears the armor, the armor wears the addon, the addon points to your custom mesh". On disk it is `NPC_.WNAM` → ARMO.
  - **What the engine does instead.** It never reads the field. Every NPC's intrinsic skin layer is its race's `WNAM`, so an authored per-actor body (its mesh and texture set, through the ARMO → ARMA chain) is replaced by the race default.
- **Evidence**: byte walk of `Skyrim.esm`.
  - 664 of 5,118 NPC_ records author `WNAM` on the shell.
  - Resolving each NPC's skin through its Use-Traits chain (ACBS template flag `0x0001` + `TPLT`, as the CK does) gives **442** NPC_ records whose effective skin ARMO differs from their race's `WNAM`. Another 1,118 records template off an LVLN and were not resolved statically, so 442 is a floor.
  - The six Alduin records are `AlduinBase`, `MQ101Alduin` (the Helgen intro on the MQ101 route), `MQ106Alduin`, `MQ206Alduin`, `MQ206AncientAlduin` and `MQ304Alduin`.

  Largest groups:

  | Skin ARMO | NPC_ records |
  |---|---|
  | `SkinDraugrMale05` / `02` / `04` / `03` / `07` / `01` | 73 / 66 / 66 / 51 / 34 / 7 (297 total) |
  | `SkinFalmer01`–`06` | ~34 |
  | Horse hides (Black, Palomino, BlacknWhite, Grey, …) | ~20 |
  | `SkinWolfBlack` / `SkinWolfSummon` | 16 / 4 |
  | `SkinSkeletonNecro*` (3 variants) | 12 |
  | `dunLabrynthianDraugrArmorFX` | 8 |
  | `skinDragonAlduin` | 6 |
  | `SkinFrostbiteSpiderCold` | 5 |
  | `SkinMammothBranded`, `SkinMagicAnomaly` | 3, 3 |
- **Impact**:
  - The Draugr population's authored body variety collapses to the single `SkinDraugr`.
  - Alduin, the main-quest antagonist, renders with the generic dragon race skin.
  - Falmer variants, black wolves, branded mammoths, necromancer skeletons and the Labyrinthian FX draugr all lose their authored look.
  - The bug is silent: the race skin still resolves, so every equip guard and the m41 smoke stay green.
  - Mechanism, not just data: modded NPC skin overrides, the standard way Skyrim mods re-skin actors, are dropped the same way.
- **Related**: #2093 (RACE `WNAM`), #3408 / SKY-D3-2026-10-05-01 (the creature skins' `BODT` masks), #4092 / #4812 (Use-Traits / TPLT terminal resolution), #4457 (`ResolvedNpc`).
- **Suggested Fix**:
  1. Decode `NPC_.WNAM` (remapped FormID) onto the NPC record.
  2. Resolve it through the Use-Traits terminal of `ResolvedNpc`, the same terminal race and gender come from.
  3. In `build_npc_equip_state`, prefer it over `race.default_skin` as the intrinsic skin layer.
  4. Add a real-data guard: a `SkinDraugrMale05` Draugr resolves that ARMO, and Alduin resolves `skinDragonAlduin`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

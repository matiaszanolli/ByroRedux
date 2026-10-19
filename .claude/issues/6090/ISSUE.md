# FO3-SD-04: The RACE body-section ICON skin is never applied on FO3

Labels: medium,character,nifal,bug,game:fo3,legacy-compat

**Severity**: MEDIUM · **Dimension**: 5 (NPC body assembly) · **Game**: FO3 (FNV hit the same way, not separately censused)

## Description
The Oblivion arm of #5487 paints the RACE body-section `ICON` over the body NIF's embedded skin. FO3 authors the same data and the parser already collects it (`body_part_textures`), but the index mapping exists only for Oblivion, so every FO3 body piece keeps the skin texture baked into the NIF. The doc comment on `body_part_textures` states the opposite of the data.

## Evidence
Location: `byroredux/src/npc_spawn.rs:689-702` (`humanoid_body_path_icon_index` returns `None` for every game but Oblivion), `byroredux/src/npc_spawn/resumable/runtime.rs:249-254` and `:131-147` (`race_body_texture` only reached through it), `crates/plugin/src/esm/records/actor/race.rs:85-94` (doc: "FO3 / FNV author body textures in the head-section table instead; this list is empty there"). Re-confirmed present at HEAD.

- `Fallout3.esm` RACE records carry `ICON` under `NAM1` (the body section): `Ghoul` -> `Characters\Ghoul\UpperBodyMale.dds`, `Characters\Ghoul\HandMale.dds`; the four `*Raider` races -> `Characters\Raider\UpperBodyMale.dds`, `HandMale.dds` (female: `RaiderFemale`); the four `*Child` races -> `Characters\Child\UpperBodyMale.dds`, `Characters\ChildFemale\...`. All 12 textures exist in `Fallout - Textures.bsa`. FO3 body indices: 0 upper body, 1 left hand, 2 right hand (not Oblivion's 0 / 1 leg / 2 hand / 3 foot).
- The body NIFs embed the default skin: `upperbody.nif` -> `characters\male\UpperBodyMale.dds`; `lefthand.nif` -> `characters\male\HandMale.dds`; **`childupperbody.nif` -> `characters\male\UpperBodyMale.dds` (adult male)**; `femaleupperbody.nif` -> `characters\female\UpperBodyFemale.dds`.
- Placed humanoids whose resolved race has a different body ICON: raiders 417 of 3,252 ACHR, ghouls 122, children 62: **~600 (18%)**. FNV's RACE records author identical ICONs for Raider, Ghoul and Child.
- The head already gets its race ICON (`head_part_texture`, `runtime.rs:294-295`); the ghoul teeth / tongue ICONs are unused too.
- Disproof attempted: armour covers most raiders but exposed hands/neck/head remain and the child, ghoul and underwear cases are naked-body; no second mechanism recolours skin (`body_textures` is only the Oblivion fill and the Body phase consumer). Not verified live.

## Impact
Ghoul NPCs have human bodies under ghoul heads, children wear adult male skin, raiders have default instead of tanned skin.

## Related
#5487 (closed; Oblivion only), #5783, FO3-SD-10 (the `race.rs` doc).

## Suggested Fix
Add a FO3/FNV mapping (0 upper body, 1 left hand, 2 right hand) in `humanoid_body_path_icon_index`; `upperbody.nif` also carries underwear and gore-cap meshes, so apply the texture to the skin mesh only (the Body phase currently sets `base_color` on every mesh of the NIF).

Source: docs/audits/AUDIT_FO3_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)

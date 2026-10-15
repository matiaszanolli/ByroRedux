# OBL-D5-2026-10-14-04: `GetButtonPressed` answer is keyed by the raising quest; the CS wiki contract is global

**Labels**: low,game:oblivion,legacy-compat,scripting,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D5-2026-10-14-04`. Re-verified against current code before filing.

- **Severity**: LOW (latent; unreachable today)
- **Dimension**: 5
- **Location**: `messages::{ButtonPresses, record_button, take_button, show_obscript_message}` (`source: message.message_box.then_some(quest)`)
- **Status**: NEW
- **Description**: `take_button(world, quest)` only sees boxes that same quest raised. The CS wiki (`GetButtonPressed.wiki`) says the first call after a button press returns the button number and otherwise -1, with no per-script scope. Vanilla quest scripts `MQ01Script` (4 reads) and `Dark02WateryScript` read it without raising any box themselves; their boxes come from stage result scripts or object scripts, which the engine does not run yet. Vanilla quest scripts that raise boxes (MS43, Vampire ×13, FGConversation…, Dark00General, Dark01Knife, DarkExileControl, SE04, SE14, MQ14, TG11Heist, FGD08Infiltration, Dark16Kiss, Charactergen) never read the button, so persisted locals cannot strand them.
- **Evidence**: census over `Oblivion.esm` quest scripts (SCRI of 390 QUST) for `getbuttonpressed` / `0x1000` / `0x1059`.
- **Impact**: none now. When stage-result or object scripts start raising boxes, a reader in a different script or quest will see -1 forever.
- **Suggested Fix**: when result scripts land, make the press a single global slot (last answered box) consumed by the first `GetButtonPressed` after it, keeping `source` only for routing.

## Known-open items, re-verified

- #5505 — still OPEN; the LOD half is stale (OBL-D4-2026-10-14-02), the parent-LAND ring remains.
- #5526 — still OPEN (no `VTCK` / `VTYP`; voices ship as race\sex `.mp3`).
- #5350 — still holds (latent: `topic_entry` has no Oblivion arm; nothing observes it).
- #5752 — still OPEN, partly overtaken: the catalog is now 48 functions and fn 79 / 237 are modeled on Oblivion.
- #5750 (Skyrim twin of OBL-D2-2026-10-14-01) — OPEN.
- #5753 (Argonian / Khajiit tailless) and #5754 (stale `humanoid_skeleton_path` doc) — both still hold.
- #5439 — **closed and fixed** (`5f414d681`); verified live. The skill lists it as known-open: stale (see below). Residual: OBL-D1-2026-10-14-01.
- #5011 (EGM sweep) and #5351 (HNAM doc rot) — not re-examined this run.
- Fixed items re-checked and holding: #5504, #5073, #5397, #4638, #5013, #5084, #5400, #5377, #5033, #5696 (ARMO BMDT `u16`), #5463.

## Regression Guard List

Stride-drift family (#1506–#1509, the four-gate corpus lane above); #170 dual band (corpus + one-sided unit pin); `NiTexturingProperty` u32 shader-map count (#149); morph cutoff (#1509); BSA v103 all-17 sweep and the zero-size placeholder tests (#5073); `parallax_alpha_gate_tests`; the Disney gate stays 0 for Oblivion (re-measured over 41 886 imported meshes: `is_pbr`, `material_path`, `from_bgsm` and translucency all 0, so `MAT_FLAG_PBR_BSDF` stays 0); census figures that reproduce exactly (1 274 `APPLY_HILIGHT2` properties, 100 with a BC1 `_n.dds`; apply modes 1 and 3 on 681 of 30 121; 14 `bump_texture` uses; dark-map role on 8 meshes / 6 files).

## Open Work

From `ROADMAP.md` Known Issues. Interiors and the Tamriel exterior already render, so neither is a blocker. Items that bear on Oblivion: the full-detail parent LAND for child worldspaces (#5505), Oblivion voice resolution (#5526), the opcode-to-name catalogue for TES4 conditions (OBL-D2-2026-10-14-01, #5752), object-script blocks and actor-state functions (M47.3 phase 2), tail MODL on Argonian / Khajiit (#5753), and an unverified-on-device ground-cover change (below).

## Needs on-device verification (not run)

No Vulkan process was launched. Not run: `m-exteriors.sh oblivion …`, `/audit-runtime --game oblivion`, the Gilded Carafe runtime baseline, `p0-door-interaction.sh oblivion`, the HUD smoke `m48-4-oblivion-hud.sh`, `cell_climate_probe`. Visible-behaviour questions:
- **Ground cover is live on Oblivion** (`592f31b87`, `c14dd2b39`): 75 of 229 LTEX author `GNAM` (166 refs, all resolving) and 108 GRAS are all placeable. Cover is now localised, so the 154 LTEX without `GNAM` grow nothing. That matches the authored data but is a visible change; not rendered.
- `_far.nif` placement LOD and child-world terrain LOD (BravilWorld).
- Interior Show-Sky skies in the realm cells (OBL-D4-2026-10-14-01).
- POM zeroing, seam-blend tone and the dark-map multiply (verified by code and tests only).

## Stale skill premises (for the next `/audit-oblivion` sync)

- **Preamble:** #5439 is closed and fixed; move it to "Fixed since" with the residual note (OBL-D1-2026-10-14-01). The condition catalog is 48 functions, not 24.
- **Dim 1:**
  - The #4257 pins are `pre_gamebryo_version_with_blocks_uses_inline_names`, `gamebryo_version_never_uses_inline_names_even_with_an_empty_table` and `zero_blocks_never_uses_inline_names_regardless_of_version` (`crates/nif/src/tests.rs`), not `uses_inline_block_type_names`.
  - The #3926 pins are in `crates/nif/src/tests.rs` (`a_median_smaller_than_the_failed_block_s_own_consumption_is_declined`, `a_median_larger_than_the_failed_consumption_still_recovers`); `oblivion_skip_sizes` has no production caller.
  - The #149 pin builds a v20.2.0.7 / bsver 83 header, not an Oblivion one.
  - Add the corpus census as the "off-band version" baseline: 16 header tuples; 8 stale-tail `landscape\lod\60.*` files.
- **Dim 2:**
  - "0 `PNAM` / 0 `ANAM`" holds for `Oblivion.esm` only; the DLC `.esp` files carry hundreds of PNAMs (Knights 868, BattlehornCastle 120, ThievesDen 41).
  - The section should mention #5734 quest-level gates (339 of 390 quests carry one) next to the QSTI / Random-stack bullet.
  - The ARMO bullet should say the BMDT mask is the `u16` and that the General Flags byte (Hide Rings / Amulets, Non-Playable bit 6, Heavy bit 7) is decoded nowhere for Oblivion ARMO / CLOT, so `armor_type` is `None` on all 996 armours.
- **Dim 3:**
  - "15 / 15 races author 8" is wrong: 13 races author 8; Khajiit and Argonian author 10.
  - The swap runs in the per-spawn `pre_spawn_hook` on a copy-on-write clone (`apply_spawn_hook` → `Arc::make_mut`), not "at import".
  - Add `byroredux/src/npc_spawn/resumable/runtime.rs` and `crates/plugin/src/esm/records/actor/race.rs` to the Paths.
  - "140 of 208 emitter-bearing NIFs carry several `NiPSysEmitter`s": measured 87 of 140 in `Oblivion - Meshes.bsa` and 144 of 214 across base + DLC; 140 is the emitter-bearing count.
- **Dim 4:** the land / LOD bit claim is stale (LOD consumed since #5496). Add the live, localised ground-cover fact (75 of 229 LTEX) with `/audit-exterior` as owner, and the WTHR `HNAM` size (56 bytes, 37 / 37).
- **Dim 5:**
  - `Message` / `MessageBox` / `GetButtonPressed` are no longer phase 2; only object-script blocks and actor-state functions remain.
  - Quest locals persist (saved `ObScriptQuestLocals`, FORMAT 97), and fn 79 `GetQuestVariable` is evaluated on Oblivion / FO3 / FNV; `is_modeled` replaces the four copies of the unknown-function fail-closed test.
  - The `HudBar` `0x3E8` doc rot is fixed (`468c66142`, #5033).
  - `menu_texture_header_census_decodes` now also pins per-set counts 1 975 / 1 712 / 1 789 (#5669), and `vanilla_corpus_parses` pins 1 868 / 2 773 tiles (#5329).

## Process notes

**Dedup.** Open-issue titles searched for dialogue / condition / quest-gate, XCCM, climate, `GetQuestVariable`, `GetButtonPressed`, LOD. Related existing: #5752 (INFO-level, distinct from the quest-level OBL-D2-2026-10-14-01), #5750, #5505, #5526, #5350, #5753, #5754. Closed and not regressed: #5439, #5463, #5734 (accepted fallout), #5696, #5487.

**Dropped candidates (by dimension agents).** Barter (`61ad95cf3`) is off the Oblivion path (no Oblivion FACT carries VEND / VENC / VENV, and the `attach.rs` change is a visibility change). The `attach.rs` hunks from `5f1671e59` and `74a05df6e` are VMAD / Papyrus-only. The menuxml fixes (#5329, #5668, #5670, #5693, #5669) show no defects. The #149 pin running on a Skyrim header still discriminates the bug it guards. A runaway float overflow to inf / NaN in `ObScriptQuestLocals` could break the JSON save column on load: routed to `/audit-save` as a hardening note, not filed here. Gameplay effect of the new Essential / Protected stamping on Oblivion NPCs: routed to `/audit-gameplay`. `triangle.frag` ReSTIR / GI changes and the fog-colour decode are shared renderer code, covered by `AUDIT_RENDERER_2026-10-14.md`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix

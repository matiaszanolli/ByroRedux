# #5487: OBL-2026-10-09-D3-01: Oblivion NPC bodies are assembled from FO3/FNV paths. The hands are requested as `lefthand.nif` / `righthand.nif`, which Oblivion does not ship; `lowerbody.nif` / `foot.nif` are never requested; and the RACE body-section…

**Labels**: bug, game:oblivion, gameplay, high, legacy-compat

**Source**: `docs/audits/AUDIT_OBLIVION_2026-10-09.md` — finding `OBL-2026-10-09-D3-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: HIGH. Visible actor geometry is missing on essentially every Oblivion humanoid spawn, under ordinary conditions, with no workaround.
- **Dimension**: Legacy-Property Rendering Path / NPC spawn + `seam_blend`. The mechanism belongs to `/audit-gameplay`; the Oblivion data is owned here.
- **Location**:
  - `byroredux/src/npc_spawn.rs:640-649`: `humanoid_body_paths`, the Oblivion arm shared with FO3/FNV.
  - `byroredux/src/npc_spawn.rs:661-673`: `humanoid_body_path_biped_mask`. It has only Oblivion `lefthand`/`righthand` → `1 << 4`, with no lower-body or foot arm.
  - `byroredux/src/npc_spawn/resumable/runtime.rs:633-638`: an archive miss logs at `debug` and is skipped silently.
  - `crates/plugin/src/esm/records/actor/race.rs:507`: `b"ICON" if in_head_section`, so body-section ICONs are dropped.
  - `byroredux/src/npc_spawn/resumable/runtime.rs:1241-1290`: `build_seam_context` reads the neighbour texture from the cached body import.
  - Tests that pin the wrong paths, in `byroredux/src/npc_spawn/tests.rs`:
    - `:612` `body_paths_kf_era_include_separate_hand_meshes` (it loops over `GameKind::Oblivion`).
    - `:665` (Oblivion female).
    - `:675` `kf_body_piece_masks_follow_each_games_hand_layout`.
- **Status**: NEW.
  - `gh` all-states searches for "Oblivion hand.nif", "handless Oblivion", "lowerbody.nif", "Oblivion body part meshes" and "race body texture" find no match.
  - #793 (closed) fixed FNV only. Its commit `da8d7e216` says for Oblivion: "needs verification … If Oblivion ships hands at different paths the load will silently miss (debug-logged)".
  - #3419 (closed) is the head/body section collision, which is a different defect.
- **Description**:
  - **Meshes**:
    - `Oblivion - Meshes.bsa` (20,182 entries, listed with `tools/bsalist.py`) ships under `characters\_male\` exactly `upperbody`, `lowerbody`, `hand`, `foot` and their `female*` twins (plus the skeletons).
    - It has **no** `lefthand.nif` / `righthand.nif`. The only `*hand*.nif` files are creature parts: zombie, and the SI gatekeeper.
    - `import_probe` on the extracted files:
      - `hand.nif` is one skinned `NiTriShape` "Hand" with both hands (x ±59, 18 `Bip01 L…` and 18 `Bip01 R…` name strings, `HandMale.dds`).
      - `lowerbody.nif` has 2 meshes (`LegMale.dds`, `GroinMale.dds`).
      - `foot.nif` has 1 mesh (`FootMale.dds`).
    - The engine asks for the two missing hand files, never asks for the lower-body or foot meshes, and has no biped mask for slot 0x08 (Lower Body) or 0x20 (Foot).
  - **Skins**:
    - The Oblivion RACE body section (after `NAM1`) authors per-race skins as ICON index 0 UpperBody, 1 Leg, 2 Hand, 3 Foot, 4 Tail. Examples: `Characters\Argonian\Male\UpperBodyMale.dds`, `Characters\Khajiit\…`, `Characters\Orc\…`, `Characters\DarkElf\…`. Redguard reuses the Imperial skin.
    - The body NIFs all author Imperial textures.
    - `parse_race` reads ICON only inside the head section, so a beast or mer torso keeps the Imperial skin.
  - **`seam_blend`**:
    - The head's own texture is the race head ICON (`state.head_texture`).
    - The neighbour texture comes from the cached `upperbody.nif` import, which is the Imperial skin.
    - So for every race whose body skin is not Imperial's, the neck tone ratio is computed against the wrong skin. It is clamped to [0.5, 2] and fades over 4 units. This is the "wrong neighbour" case the skill's `seam_blend` checklist exists for.
  - **Missing hand seam**: the hand seam pass (`is_hand_part`) never runs on Oblivion, because no hand mesh loads.
- **Evidence**: census `tools/body_slots.py` over direct CNTO items (LVLI counted as unknown):
  - **Hand slot (0x10)**: covered on 769 NPC_ (738 placements), not covered on 254 (212 placements), and LVLI-dependent on 1,459 (1,240 placements). Gauntlets and gloves are rare in Oblivion leveled outfits.
  - **Lower Body (0x08)**: 87 NPC_ have no covering item.
  - **Foot (0x20)**: 159 NPC_ have no covering item.
  - Both of those miss as well, and the player body has the same gaps.
- **Impact**:
  - Hands are missing on most Oblivion humanoids. Legs and feet are missing wherever clothing does not cover them, which includes naked or partly dressed prisoners, beggars and bandits.
  - Beast and mer torsos render human skin.
  - Necks of non-Imperial-skinned races are tone-shifted.
  - Two unit tests assert the nonexistent paths, so the gap is pinned rather than guarded.
- **Related**: #793, #3419, #4794 (its "four hand-NIF re-parses" are Oblivion misses), `/audit-gameplay` NPC spawn, `/audit-fnv` Dim 4 (`seam_blend` checklist).
- **Suggested Fix**:
  - Give Oblivion its own arm:
    - Male: `[upperbody, lowerbody, hand, foot]`.
    - Female: the `female*` twins.
    - Masks: 0x04 / 0x08 / 0x10 / 0x20.
  - Decode the body-section ICONs (index 0..=4, per gender) and swap them in at body import, before `build_seam_context` reads the neighbour textures.
  - Replace the two tests with a real-data pin that every Oblivion body path exists in `Oblivion - Meshes.bsa`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix

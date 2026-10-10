# #5526: OBL-2026-10-09-D5-01: Phase V dialogue voice never resolves on Oblivion. The lookup needs NPC `VTCK` → `VTYP` and composes the FNV `.ogg` / 10-25-truncation name, but Oblivion authors neither record and ships its voices as race\sex\*.mp3. Every…

**Labels**: audio, bug, dialogue, game:oblivion, legacy-compat, low

**Source**: `docs/audits/AUDIT_OBLIVION_2026-10-09.md` — finding `OBL-2026-10-09-D5-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW (enhancement). The subtitle-estimate fallback works, and no wrong behaviour results.
- **Dimension**: Gameplay & UI Data Slice. The mechanism is owned by `/audit-gameplay` / `/audit-audio`.
- **Location**:
  - `byroredux/src/systems/dialogue_voice.rs:235-246`: the voice-type resolve.
  - `byroredux/src/systems/dialogue_voice.rs:71-100`: `voice_path_candidates`, which hard-codes `.ogg`, the FNV truncation and the voice-type folder.
  - `byroredux/src/systems/dialogue_voice.rs:1-8` and `docs/engine/dialogue-trees.md:178-188`: both call the FNV shape "Bethesda's authored convention".
- **Status**: NEW.
  - `gh` searches for "Oblivion voice", "mp3 voice" and "Phase V voice" find #5433 (open, a sibling: the FO3 profile mounts no sound archives) and #5395/#5393 (closed, FO3/FNV naming).
- **Description**:
  - `Oblivion.esm` authors 0 `VTCK` on its 2,482 NPC_ and 0 `VTYP` records, so `index.voice_types.get(npc.voice_form_id)` is always `None`. The lookup returns at `debug` level.
  - The authored convention, from `Oblivion - Voices1/2.bsa` (77,780 entries: 38,898 `.mp3` and 38,882 `.lip`), is `sound\voice\oblivion.esm\<voice race full name, lowercased>\<m|f>\<quest EDID>_<topic EDID>_<formid 8-hex>_<n>.mp3`. Example: `…\high elf\f\dark17following_greeting_0000566f_1.mp3`.
    - The race folders are argonian, breton, dremora, high elf, imperial, nord and redguard. Other races borrow a folder through RACE `VNAM` voice races. That field is already decoded for Oblivion as `RaceRecord::voice_forms` (`race.rs:591`) but has no consumer.
    - Names are **not** truncated: quests run up to 22 characters, and quest + topic up to 46.
- **Impact**:
  - No Oblivion line is voiced, including the generic greetings that every activation now opens.
  - The module doc presents the FNV rule as universal, so an Oblivion implementer gets no warning.
- **Related**: #5367 Phase V, #5433, #5395, #5393, OBL-2026-10-08-D2-01 / #5397 (Phase G liveness).
- **Suggested Fix**:
  - Add a per-game voice-path table entry for Oblivion: the speaker's race, mapped through `RACE.VNAM` (by sex) to the voice race's FULL name; the `m`/`f` folder; no truncation; `.mp3`.
  - Until then, record Oblivion as out of scope in `dialogue_voice.rs` and dialogue-trees.md.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix

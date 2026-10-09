# #5429: FO3-2026-10-08-D5-01: #5367's greeting / line-lifetime / force-greet / voice layers are documented "FO3/FNV", but every guard is FNV-only — FO3 is assumed, not verified

**Labels**: low,gameplay,dialogue,ai,bug,test-gap,game:fo3,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5429

**Source**: `docs/audits/AUDIT_FO3_2026-10-08.md` — `FO3-2026-10-08-D5-01` (HEAD `00f580e09`)

**Publish note**: The FNV dialogue floor `dialogue_greeting_and_forcegreet_fnv_floor` now starts at `crates/plugin/tests/parse_real_esm.rs:5090` (report cited `:5082-5089`; line drift only).

- **Severity**: LOW (test gap).
- **Dimension**: Animation, NPC Spawn, Gameplay Data. The mechanism owner is `/audit-gameplay`; the smoke contract belongs to `/audit-tooling`.
- **Location**:
  - `crates/plugin/tests/parse_real_esm.rs:5082-5089` (`dialogue_greeting_and_forcegreet_fnv_floor`, which reads `FalloutNV.esm` only);
  - `docs/smoke-tests/dt1-dialogue-layers.sh:65` (`--game fnv --cell GSDocMitchellHouse`, the only live gate);
  - the claims in `ROADMAP.md:321` ("per-line voice playback on the FO3/FNV path") and `docs/engine/dialogue-trees.md:47`, `:75`, `:145` (an FO3 census row; "force-greet (FO3/FNV …)").
- **Status**: NEW. GAME-D2-2026-10-08-04 covers the missing FO3 sound archives (a profile defect), and FNV-2026-10-08-D6-01 covers dt1's SKIP contract. Neither notes that no FO3 guard exists for Phases G, L or F.
- **Description**: The four #5367 layers ship as an FO3/FNV feature, and `dialogue-trees.md` even records a `Fallout3.esm` census row (6,381 DIAL / 22,327 INFO / 374 greeting DIALs / 386 Dialogue PACKs). None of the guards loads FO3 data:
  - the real-data floor asserts FNV counts only;
  - the live gate boots FNV only;
  - the only "fallout3" mention in the dialogue modules is one synthetic `voice_path_candidates` unit case (`dialogue_voice.rs:235`).

  That is the divergence class this audit exists for: verified on FNV, assumed on FO3. It already hides two FO3-side defects that sibling audits found by reading code: no FO3 sound archives (GAME-D2-04) and FO3's ~42% voice-name truncation miss (LC-D3-01). The FO3 premises the floor would pin are measured above and all hold today: `GREETING` literal EDID at 0xC8 with 2,542 INFOs, `HELLO` at 0xD2, 386 procedure-15 PACKs, and QSTI on 22,327 / 22,327 INFOs. Nothing would catch them rotting.
- **Evidence**: `git grep -i "fallout3\|fo3"` over `npc_dialogue.rs`, `forcegreet.rs`, `dialogue_voice.rs`, `eat_sleep.rs`, `dt1-dialogue-layers.sh`, `m42-eat-sleep.sh` and the dialogue floor finds doc comments and the one synthetic voice-path case only.
- **Impact**: An FO3-only regression in the greeting lookup, the INFO flag reading, the PKDD topic decode or the voice path passes CI and every gate.
- **Related**: GAME-D2-2026-10-08-04, LC-D3-01, GAME-D5-2026-10-08-01 (the FO3 addendum above), FNV-2026-10-08-D6-01, #5367.
- **Suggested Fix**:
  - Add a `Fallout3.esm` leg to the dialogue floor: `GREETING` exists with at least 2,500 INFOs, at least 380 procedure-15 PACKs, and every INFO carries QSTI.
  - Once GAME-D2-04 is fixed, add an FO3 voice-resolution unit case built from a real FO3 file name.
  - Add an FO3 leg to `dt1` (or declare a `dt1` gate in `fo3.env`) with the SKIP=77 path.

## Completeness Checks
- [ ] **SIBLING**: FO3 legs added for the real-data floor, the voice-path unit case and the `dt1` live gate (or an `fo3.env` gate declaration)
- [ ] **TESTS**: A regression test pins this specific fix

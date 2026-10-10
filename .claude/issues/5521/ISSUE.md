# #5521: FO3-2026-10-09-D5-01: The voice-file name rule is "truncate only when quest + topic > 25", not "always cut quest to 10; radio quests keep the full shape"

**Labels**: audio, bug, dialogue, game:fnv, game:fo3, gameplay, legacy-compat, low

**Source**: `docs/audits/AUDIT_FO3_2026-10-09.md` — finding `FO3-2026-10-09-D5-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW. Every line resolves through the two-candidate order; the rule and its documentation are wrong, and the extra probe is waste.
- **Dimension**: Gameplay Data. The mechanism owner is `/audit-gameplay` (`dialogue_voice.rs`). Labels: `game:fo3`, `game:fnv`, `dialogue`, `audio`, `doc-rot`.
- **Location**:
  - `byroredux/src/systems/dialogue_voice.rs:10-15`: the module doc, "Radio quests (RadioNewVegas, GNR) keep their full EDIDs, so the untruncated shape stays second".
  - `dialogue_voice.rs:71-101`: `voice_path_candidates` always cuts the quest to 10 first.
  - `dialogue_voice.rs:341`: the test `voice_path_candidates_truncate_long_edids` pins that order.
  - `docs/engine/dialogue-trees.md:184-186`: the #5395 rule text, including the "radio fallback".
- **Status**: NEW. FNV-2026-10-09 measured the resolution *rate* (96.9%), not the rule. No gameplay finding covers it.
- **Description**: the module doc says the convention was "pinned against … FO3's `Fallout - Voices.bsa`". Measured against that archive, the authored rule is a clean length test.
  - **FO3**: `py/voice_rule.py` uses the first response of each INFO, counting only the lines where the engine's cut differs from the full EDIDs.
    - All 5,442 truncated-shape files have `len(quest) + len(topic)` in 26..61.
    - All 3,592 full-shape files have it in 14..25.
    - Those 3,592 span **55 quests**, led by `DialogueRivetCity` (469), `GenericRobot` (363), `ConvMegaton` (286), `DialogueMegaton` (243) and `DialogueTenpenny` (213). None of the top twelve is a radio quest.
  - **FNV** (`Fallout - Voices1.bsa`) has the same partition: 16,876 cut (26..93) and 3,961 full (14..25) across 80 quests. `RadioNewVegas` is 121 of those lines.
- **Impact**: resolution is unaffected, because both shapes are tried. However:
  - 3,592 / 11,955 FO3 lines (30%) and 4,207 / 28,768 FNV lines (15%) pay a guaranteed-miss archive probe first.
  - The documented rationale is false.
  - The unit test pins the guess rather than the rule.
- **Related**: #5393, #5395 (closed), #5433, #5429.
- **Suggested Fix**:
  - Emit one candidate: `if q.len() + t.len() > 25 { (q[..10], t[..15]) } else { (q, t) }`.
  - Rewrite the doc and the test around the length rule.
  - Add one real-name case per game, for example `dialoguerivetcity_greeting_…`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix

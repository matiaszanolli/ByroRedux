# #5464: SKY-D5-2026-10-08-01: Skyrim `.fuz` is documented as "FUZE + RIFF lip + XMA2", but every SE and LE file is FUZE + raw `.lip` + RIFF **xWMA** — the claim is now steering the V2 decoder scope

**Labels**: low,audio,documentation,doc-rot,test-gap,game:skyrim,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5464

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-08.md` — `SKY-D5-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: LOW
- **Dimension**: 5 — Archives + corpus gates (Skyrim voice archive content)
- **Location**:
  - `crates/bsa/tests/bsa_real.rs:497-540`: doc lines 501-502 say "XMA2 on LE … Decoding XMA2 is its own project"; lines 535 and 537 label the first RIFF "lip chunk".
  - `docs/engine/dialogue-trees.md:186-187`.
  - The `ROADMAP.md:321` M43 row ("Skyrim+ `.fuz` (FUZE + RIFF lip + XMA2) stays V2 scope").
- **Status**: NEW. AUDIT_AUDIO_2026-10-08 (line 224) repeats the premise ("needs an XMA2 decoder") and has not filed it as a defect.
- **Description**: The FUZE container is magic `FUZE`, u32 version, u32 lip size, then the raw `.lip` bytes, then the audio. The test's `windows(4).position(RIFF)` lands on the audio chunk at `12 + lip_size`, not on lip data, and that audio chunk is `RIFF…XWMA`. XMA2 is the Xbox 360 codec, and no PC file carries it.
- **Evidence**: `fuzcensus.py` (an independent BSA v104 / v105 reader).
  - SE `Skyrim - Voices_en0.bsa`: 3,000 / 3,000 sampled `.fuz` are `FUZE` v1 + RIFF/XWMA, with the first RIFF at `12 + lip_size`.
  - LE `Skyrim - Voices.bsa`: identical, 3,000 / 3,000.
  - The test's own sample, `dialoguege_dialoguegeneric_0006ce5e_1.fuz`, has lip_size 1474 and `RIFF … XWMAfmt` at 1486.
- **Impact**:
  - **Decoder choice.** The V2 voice plan targets the wrong decoder: xWMA (WMA v2 / WMA Pro in RIFF) is needed, not XMA2.
  - **Test shape.** The shape test passes while recording a false layout, so it cannot catch a reader that mistakes the audio for the lip track.
- **Suggested Fix**:
  - Parse the `lip_size` header field in the test.
  - Assert that `RIFF` sits at `12 + lip_size` with a `XWMA` form type.
  - Correct the three doc sites and tell `/audit-audio` the codec is xWMA.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (AUDIT_AUDIO_2026-10-08 XMA2 premise; audio-subsystem docs)
- [ ] **TESTS**: A regression test pins this specific fix (`skyrim_fuz_container_shape` asserts `RIFF`/`XWMA` at `12 + lip_size`)

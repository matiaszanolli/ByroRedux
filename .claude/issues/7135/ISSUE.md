# #7135: PAR-D3-2026-11-20-03: no Bink 2 header flag except video bit 1 is inspected, and the video crate's `Unsupported` doc says otherwise

**Labels**: low,import-pipeline,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7135

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D3-03 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Version Gating
- **Trigger Input**: a KB2g/KB2j file whose `video_flags` (`+0x24`) carry a bit that changes the stream layout (a plane the decoder does not model), or whose audio track flags carry bits other than 12/13.
- **Location**: `crates/bink2/src/container.rs:153` (`slices()` reads bit 1 only), `crates/video/src/lib.rs:36-37` (doc: "an alpha or grayscale plane" is `Unsupported`)
- **Status**: NEW
- **Description**: `Bk2` stores `video_flags` and uses only bit 1. There is no refusal for any other bit. STATE.md lists alpha planes as unprobed and README says they are "Not decoded", but nothing makes that an `Unsupported`; the `VideoError::Unsupported` doc and the PRD wording imply the video crate refuses an alpha/grayscale plane, which is true only on the Bink 1 path (`mercs2_bink::VideoDecoder::new`, probe2: `video flags 0x100000: alpha planes are not supported`). CLEANROOM.md forbids taking the bit value from the NihAV exposure, so the fix needs its own probe.
- **Evidence**: probe3 toggles each of the 30 other bits on a real KB2g and a real KB2j file: all 6 sampled frames hash identically to the unmodified file for every bit (0, 2..31). Audio: `Decoder::new` receives only rate/stereo/dct.
- **Impact**: none for the 54 corpus files (all exact versus the DLL). A mod movie with a plane the decoder lacks would be decoded with the wrong stream model: most likely a desync `Err` (movie ends `Failed`), possibly wrong pixels.
- **Related**: #6907 (provenance), STATE.md open item 1.
- **Suggested Fix**: probe which video-flag bits real alpha files set against the oracle (not from the disclosed third-party source), then refuse them in `Bk2::open`; until then correct the `VideoError::Unsupported` doc to say the alpha/grayscale refusal is Bink 1 only.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix

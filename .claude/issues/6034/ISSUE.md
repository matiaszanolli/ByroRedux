# REN-EXT-L-05: doc rot after the grass-LOD rewrite (`af8323881`)

Labels: low,renderer,documentation,doc-rot
Source: docs/audits/AUDIT_RENDERER_2026-10-18.md

- **Severity**: LOW (doc rot)
- **Dimension**: Debug/Telemetry (docs)
- **Location**:
  - `groundcover/frame.rs`, comment in `record_draw`: "Stream 0 is the three-segment tier and stream 1 the one-segment tier" (there are three streams, 3/2/1 segments).
  - `groundcover/frame.rs`, comment in `record_scatter`: "the blade tier is `segments * 6`" (the live formula is `3 * (2N - 1)`).
  - `groundcover.rs`, `BladePush` doc: lane `w` (the tier index) is not described.
  - `docs/engine/exal-groundcover.md` status header: still says "Tier 2 palette-atlas clump cards and their area fade IMPLEMENTED (2026-09-15)", while the same file marks that tier *(removed)* further down.
- **Status**: NEW
- **Suggested Fix**: edit the comments and the header sentence.

_Filed from `docs/audits/AUDIT_RENDERER_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._

# EXT-D3-2026-10-18-02: `GROUNDCOVER_BLADE_FADE_START` is documented as a "long on purpose" fade but equals `GROUNDCOVER_FADE_START` since `b731b01f5`

Labels: low,terrain-exterior,shaders,documentation,doc-rot
Source: docs/audits/AUDIT_EXTERIOR_2026-10-18.md

- **Severity**: LOW (doc-rot)
- **Dimension**: Ground-cover pipeline
- **Location**: `crates/renderer/src/shader_constants_data.rs:573-579` (doc), `:579` (`= 7000.0`, same as `GROUNDCOVER_FADE_START` `:485`); `crates/renderer/shaders/include/groundcover_density.glsl:348-354` ("the procedural blade tier's own, longer fade").
- **Status**: NEW
- **Tier Violated**: n/a
- **Description**: Before the reach change the blade fade began earlier than the model fade; both now start at 7,000 and end at 8,000, so `byroGcBladeDistanceFade` and `byroGcDistanceFade` are the same function. The doc still promises "the stochastic thinning of far points starts well inside the draw distance". Behaviour is correct; the second constant and its comment are now vestigial.
- **Suggested Fix**: Either collapse to one constant or restore a distinct value deliberately; at minimum fix the comment.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._

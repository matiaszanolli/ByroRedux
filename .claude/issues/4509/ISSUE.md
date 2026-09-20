# EXT-D7-2026-09-19-09: ground-cover eval records manifest rows with absent or annihilated telemetry

- **ID**: EXT-D7-2026-09-19-09
- **Labels**: low,tech-debt,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4509

**Severity**: LOW · **Dimension**: Acceptance harness · **Game Affected**: FNV, Skyrim SE
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D7-2026-09-19-09)

**Location**: `scripts/renderer-eval-groundcover.sh:124-131`; emission at `crates/renderer/src/vulkan/groundcover.rs:334`

**Description**
`bench="$(awk '/^bench:/{line=$0} END{print line}' …)"` and the `groundcover:` equivalent yield an empty string when the row never printed, and the script writes the manifest row anyway with an empty column, exit 0. A `groundcover: chunks=0 blades=0` row — the engine's own documented "a factor annihilated the product" signature (`GroundCoverStats`, pinned by `empty_stats_render_a_well_formed_row`) — is likewise recorded without comment.

**Impact**
A ground-cover pass that silently scattered nothing produces a perfectly-formed reference manifest; the reviewer must notice a zeroed row in a TSV.

**Related**: EXT-D7-2026-09-19-01

**Suggested Fix**
Fail (or stamp the manifest) when the `groundcover:` row is missing or has `chunks=0 blades=0` on a backlit case; the engine already reports the field needed to name the annihilating factor.

## Completeness Checks
- [ ] **TESTS**: A zero-telemetry capture run stamps/fails instead of minting a clean row

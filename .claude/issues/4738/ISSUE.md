# EXT-D7-2026-09-21-02: The harnesses' stale-SPIR-V defence is still a comment and misdescribes the landed build.rs change

**Issue**: #4738
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: LOW
**Dimension**: Acceptance gates and harness
**Tier Violated**: no-fabrication (a capture attributed to shader X may be shader X−1)
**Game Affected**: all
**Location**: `docs/smoke-tests/m-exteriors.sh:102-108`, `docs/smoke-tests/w1-water-traversal.sh:89-95`, `scripts/renderer-eval-groundcover.sh:90-95`; `crates/renderer/build.rs:24-31`

## Description
All three harnesses still say the `build.rs` `rerun-if-changed` on `shaders/**` is a future structural fix "tracked separately." It already landed (`a04ccaec3`; confirmed present at HEAD `ee6d3fb39`, `build.rs:31`). It dirties the crate on shader source edits but cannot recompile a `.spv` — an edited-but-not-recompiled shader still ships stale SPIR-V. The repo's real check, `scripts/check-shader-artifacts.sh` (used by `ci.yml`), is never run by any of the three harnesses before capturing.

## Evidence
Ran `scripts/check-shader-artifacts.sh`: found `DRIFT crates/renderer/shaders/composite.frag.spv` (since fixed by #4577). Every exterior-owned `.spv` reproduces byte-identically as of this pass.

## Impact
A visual claim from any of the three harnesses can still be judged through a stale shader — the #4490 premise persists operationally even though its structural half landed.

## Suggested Fix
Call `scripts/check-shader-artifacts.sh` in each harness preflight, failing or stamping the manifest on DRIFT. Update the three comments to describe the landed `build.rs` behavior accurately.

## Related
#4490 (closed), #4577 (the composite.frag drift this pass found), #4730 (EXT-D7-01)

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D7-2026-09-21-02)

# M33-09: Four NAM0 colour groups parsed but never consumed

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/541
- **Severity**: MEDIUM
- **Dimension**: Shader / consumer
- **Audit**: `docs/audits/AUDIT_M33_2026-04-21.md` § M33-09
- **Status**: NEW (created 2026-04-21)

## Location

- `byroredux/src/systems.rs:1197-1226` (interpolation)
- `crates/renderer/shaders/composite.frag:68-167` (sky branch)

## Summary

`SKY_STARS`, `SKY_LOWER`, `SKY_CLOUDS_LOWER`, `SKY_CLOUDS_UPPER` are all parsed from NAM0 but never read by any consumer. Stars never render; below-horizon tint is faked; cloud tint lost. Gated on M33-01..M33-04 landing first.

Fix with: `/fix-issue 541`

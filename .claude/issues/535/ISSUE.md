# M33-03: DNAM misinterpreted as cloud_speeds — it's a texture path

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/535
- **Severity**: CRITICAL
- **Dimension**: ESM parser
- **Audit**: `docs/audits/AUDIT_M33_2026-04-21.md` § M33-03
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/plugin/src/esm/records/weather.rs:176-182` (DNAM arm)
- `byroredux/src/systems.rs:1277` (downstream consumer)

## Summary

DNAM arm reads 4 bytes as cloud speeds; actual DNAM is a cloud-texture zstring. `cloud_speeds=[115,107,121,92]` is literally ASCII `"sky\"`. Must fix with M33-02.

Fix with: `/fix-issue 535`

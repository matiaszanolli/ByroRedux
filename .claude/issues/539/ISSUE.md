# M33-07: parse_wthr / parse_clmt lack GameKind gate

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/539
- **Severity**: MEDIUM
- **Dimension**: Cross-game parser dispatch
- **Audit**: `docs/audits/AUDIT_M33_2026-04-21.md` § M33-07
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/plugin/src/esm/records/mod.rs:340-346` (WTHR/CLMT dispatch)
- `crates/plugin/src/esm/records/weather.rs:122` (`parse_wthr` signature)
- `crates/plugin/src/esm/records/climate.rs:56` (`parse_clmt` signature)

## Summary

Dispatch calls `parse_wthr(fid, subs)` with no GameKind. Skyrim WTHR layout differs materially; the `>= 240` gate silently accepts larger Skyrim NAM0. Latent today; goes live with M32.5 Skyrim cell-loader parity. Couple with M33-01/02/04/05 fixes.

Fix with: `/fix-issue 539`

# M33-10: Procedural no-WTHR fallback sun never sets

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/542
- **Severity**: LOW
- **Dimension**: Fallback path
- **Audit**: `docs/audits/AUDIT_M33_2026-04-21.md` § M33-10
- **Status**: NEW (created 2026-04-21)

## Location

- `byroredux/src/scene.rs:351-373` (procedural fallback insertion)
- `byroredux/src/systems.rs:1129-1144` (weather_system early-return)

## Summary

Fallback branch doesn't insert `GameTimeRes` + `WeatherDataRes`. `weather_system` skips → fallback sun frozen at `[-0.4, 0.8, -0.45]`, intensity 4.0, forever. Cosmetic today; surfaces once CRITICAL parser fixes land.

Fix with: `/fix-issue 542`

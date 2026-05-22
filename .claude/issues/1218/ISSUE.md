**Severity**: LOW (documentation drift)
**Dimension**: NIF Format Readiness
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` Dim 2 FIND-4

`CLAUDE.md:288` cites the 2026-04-26 sweep: Oblivion 95.21%, FO4 96.46%, FO76 97.34%, Starfield 97.19%.

`ROADMAP.md:165-171, 668` (the "Project Stats" ground-truth section) cites a different sweep date: Oblivion 96.24%, FO4 96.46%, FO76 97.34%, Starfield 98.6% aggregate (2026-04-27).

FO4 and FO76 agree; Oblivion drifts by ~1pp, Starfield by ~1.4pp. ROADMAP.md is the designated ground-truth (per CLAUDE.md itself).

### Impact
Future readers ask "which number do I cite?" — false-positive premise for a "regression" finding in a later audit.

### Suggested Fix
Refresh CLAUDE.md from ROADMAP.md or pick a single authoritative number per game. Probably a `/session-close` follow-up; not a code change.

### Completeness Checks
- [ ] **TESTS**: N/A.

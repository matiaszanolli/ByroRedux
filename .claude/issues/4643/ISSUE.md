# ESM-2026-09-21-D1-01: FO76 HEDR is a live-service value — installed masters now ship 279.0; code, tests, docs and the skill still pin "the real 266.0"

**Issue**: #4643
**Filed**: 2026-09-22 (audit-publish, AUDIT_ESM_2026-09-21.md)

**Severity**: LOW
**Dimension**: Header & GRUP Walk
**Game Affected**: Fallout 76
**Location**: `crates/plugin/src/esm/reader.rs:164`, `:189-213`, `:1417-1429`; `crates/plugin/src/esm/records/tests.rs:2326-2337`; `docs/engine/esm-records.md:228`; `.claude/commands/audit-esm/SKILL.md:96,218`

## Description
A 2026-09-20 game patch rewrote both installed FO76 masters' HEDR from 266.0 to 279.0 (bytes `00 80 8b 43`; TES4 rv 209 unchanged; gained an `MMSB` sub-record). Multiple comments/tests/docs/skill lines still hard-pin 266.0 as "the real value" / a settled decode.

## Evidence
`xxd` of `SeventySix.esm`/`NW.esm` (mtime 2026-09-20) gives HEDR 279.0f32. Classification unaffected — `from_header`'s floor is `>= 60.0`.

## Impact
No functional bug — comment/doc/skill rot only, for a value that has now drifted twice (68→266→279).

## Suggested Fix
Record FO76 HEDR as patch-dependent with dated samples (266.0 on 2026-08-28, 279.0 on 2026-09-20); keep the floor rationale as load-bearing. Update `esm-records.md:228` and the skill lines.

## Related
#3405 (closed — the prior 68→266 correction)

## Source
docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D1-01)

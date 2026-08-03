# OBL-D7-01: README.md still frames Oblivion exterior as wiring-gated, contradicting ROADMAP.md

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2348
**Severity**: LOW
**Dimension**: Exterior Blocker Chain & Game-Specific Quirks
**Location**: `README.md:129-130`
**Source audit**: `docs/audits/AUDIT_OBLIVION_2026-08-03.md` (finding OBL-D7-01)
**Labels**: low, legacy-compat, documentation

### Description
`README.md:129-130` reads (present tense) "Oblivion exterior gated on TES4
worldspace + LAND wiring" — implying the wiring is still the blocker.
`ROADMAP.md` (more recently touched) is explicit the wiring is done and only
the on-device render bench remains.

### Impact
Documentation friction only — risk of a future contributor or audit
re-opening "wiring missing" as if it were still live.

### Suggested Fix
Reword `README.md:129-130` to match `ROADMAP.md`'s framing: "Oblivion
exterior: worldspace/LAND wiring implemented, on-device render bench
pending."

**Severity**: LOW · **Source**: AUDIT_FO4_2026-05-30 (D5-01)

**Location**: `ROADMAP.md:726`

**Description**: ROADMAP still shows an unchecked item: `- [ ] BSBoneLODExtraData has no parser — surfaced by R3 baselines: 0/34 on FO4, 0/52 on Skyrim SE`. The parser landed in commit `782b7238` (Fix #614, 2026-04-25). The "0/34 on FO4" count means zero instances in vanilla FO4 NIFs, not a parse failure.

**Suggested Fix**: Change `- [ ]` to `- [x]` in ROADMAP.md:726.

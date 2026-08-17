# SF-D8-01: the shared slot-to-role table has zero Starfield coverage by construction

**Issue**: #3057
**Severity**: LOW
**Labels**: `low,nif-parser,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_STARFIELD_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_STARFIELD_2026-08-16.md` (Dimension 8 — shader flags / slot routing).

**Location**: `crates/nif/src/import/material/slot_role.rs`:1-29

## Description

The shared slot→role table has **zero Starfield coverage by construction**, and nothing records that.

Every arm in `slot_to_role` is keyed on Skyrim-family `bs_lighting::*` shader-type constants. Starfield's `BSGeometry`/`.mat` path never reaches the table, so no slot is routed for it — and the module header does not say so.

## Evidence

Re-verified 2026-08-17: `sed -n '1,29p' crates/nif/src/import/material/slot_role.rs | grep -ci "starfield\|sf_"` → **0**. The file's documentation discusses Skyrim and FO4 evidence only.

## Impact

Low today — Starfield materials come from the CDB rather than from `BSShaderTextureSet` slots, so the absence is arguably correct. The finding is that **nothing distinguishes "correctly not applicable" from "not yet done"**.

That matters because this sweep filed three FO4 findings (#2997, #2998, #2999) that are all "a Skyrim measurement was generalised to another game" in this exact file. A reader cannot currently tell whether Starfield is the fourth instance or a deliberate non-case.

## Suggested Fix

Add a line to the module header stating that Starfield routes materials through the CDB and deliberately does not use this table — or, if slots *are* reachable on some Starfield content, measure and route them.

## Related

- #2997, #2998, #2999 (the three FO4 slot-routing findings in this same file)
- #3053 (SF-D9-01 — the Starfield material path this table is bypassed in favour of)

## Completeness Checks
- [ ] **LEGIBLE-INTENT**: The header states whether Starfield is out of scope by design
- [ ] **MEASURED**: If any Starfield content does author `BSShaderTextureSet` slots, that is measured rather than assumed absent
- [ ] **SIBLING**: FO76 checked for the same undocumented gap
- [ ] **PATH-GATE**: `_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3057 --json state` when live state is needed.*

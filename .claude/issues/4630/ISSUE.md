# NIF-D3-2026-09-21-04: audit-nif/SKILL.md drift found while running it

**Issue**: #4630
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: LOW (audit tooling)
**Dimension**: 3 (skill text)
**Location**: `.claude/commands/audit-nif/SKILL.md`
**Status**: NEW

## Description
Four statements in the audit-nif skill no longer match the code or current process, found while running it:

(a) **Dim 3's first step** says to diff `nif_stats --tsv` against the baseline TSV. `nif_stats`' own #4265 doc says the two use different keys and must not be diffed directly. Confirmed the skill text at `SKILL.md` still instructs the direct diff.

(b) **The Starfield BLSP tail** is given in the skill (`SKILL.md:84`) as "38 B on most content but bimodal `{38, 42}`". Measured this window it is `{0, 30}` (see the report's stream-position telemetry table: `BLSP {0: 480,757, 30: 3,080}`), not `{38, 42}`.

(c) **Phase 1** (`SKILL.md:54`) says to "check the latest nightly result" before trusting a green default `cargo test` for parse rates — there is no nightly result; the lane has never executed (see NIF-D3-2026-09-21-01).

(d) **The Dim 5 checklist** (`SKILL.md:227-228`) says `bhkGenericConstraint`'s "drift is suppressed" via `is_havok_constraint_stub` — but per NIF-D1-2026-09-21-02 that path is unreachable (the type has no dispatch arm and always falls to `NiUnknown` before the stub-drift check can run).

## Evidence
`grep -n "nif_stats --tsv\|38 B\|{38, 42}\|latest nightly\|drift is suppressed\|bhkGenericConstraint" .claude/commands/audit-nif/SKILL.md` at HEAD `ee6d3fb39` returns all four cited lines verbatim, matching the report exactly.

## Impact
An auditor following the skill literally would diff incompatible key formats (a), cite a stale bimodal split (b), have no nightly result to check (c), and believe a suppressed-drift claim that's actually dead code (d). Audit-quality risk only, not a code defect.

## Related
NIF-D3-2026-09-21-01 (the missing nightly lane referenced in (c)); NIF-D1-2026-09-21-02 (the dead stub telemetry referenced in (d))

## Suggested Fix
Update `.claude/commands/audit-nif/SKILL.md`: (a) note the key-format mismatch per #4265's own doc; (b) correct the BLSP tail figures to `{0, 30}`; (c) remove or caveat the "check the latest nightly result" instruction until NIF-D3-2026-09-21-01 is fixed; (d) correct the Dim 5 checklist's `bhkGenericConstraint` claim.

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D3-2026-09-21-04)

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only fix)

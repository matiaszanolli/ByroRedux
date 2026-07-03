# #1870: OBL-D1-NOTE-01: audit-oblivion SKILL.md #1509 checklist bullet has the doghead.nif bsver-9 gate backwards

- **Severity**: LOW
- **Labels**: `low`, `documentation`, `legacy-compat`
- **Source**: `docs/audits/AUDIT_OBLIVION_2026-07-03.md` (OBL-D1-NOTE-01)
- **Dimension**: NIF Version Handling

## Location
`.claude/commands/audit-oblivion/SKILL.md` lines 110-113; code is correct at `crates/nif/src/blocks/controller/morph.rs:89-92`.

## Description
The skill checklist says doghead.nif (bsver 9) "must keep the field" — backwards. The code correctly gates on `bsver > 9` (false for bsver 9, so the field is skipped); Oblivion's bsver-11 rigs keep it. Code, comment, and test name all agree; only the checklist prose is wrong. First reported 2026-07-02, not fixed by the same-day skill refresh.

## Impact
None on runtime. Risk: a future session reading only the checklist could "fix" the code to match the wrong sentence and re-break doghead.nif.

## Suggested Fix
Amend the checklist bullet to state doghead.nif (bsver 9) must **skip** the field; Oblivion's bsver-11 rigs must **keep** it.

# Tech-Debt Batch 3: Audit-skill anchor rot

**Severity**: MEDIUM
**Audit**: docs/audits/AUDIT_TECH_DEBT_2026-05-13.md
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1040

## Findings
TD10-001/003/004/005/006/007/008/009 + TD7-017/018

## Fix
Fix specific stale anchors + sweep convert `file.rs:N` → `file.rs::symbol` across all audit-* skills.

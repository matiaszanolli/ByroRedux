# FNV-D2-03: INFO dialogue children never walked — DIAL records arrive as empty shells

**Severity:** MEDIUM | esm, legacy-compat
**Source:** `docs/audits/AUDIT_FNV_2026-04-24.md`

## Problem
`DIAL` records dispatch (mod.rs:410) but `extract_records()` filters on a single `expected_type` (mod.rs:489-510), so the **nested GRUP under each DIAL containing INFO topic responses is silently skipped**. DIAL records arrive as empty shells; every NPC response is lost.

Code comment at mod.rs:127-129 already acknowledges this is a follow-up.

## Audit fix shape
Dedicated `extract_dial_with_info()` walking DIAL sub-GRUP and pushing INFO records onto `dialogue.infos`. Cleaner than parameterizing the generic walker.

## SIBLING
QUST also has nested GRUPs (sub-quests / objectives) — same shape?

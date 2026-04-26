# FNV-ESM-9: PerkRecord is EDID/FULL/DESC/flag stub — PRKE entry points unparsed

**Severity:** MEDIUM | esm
**Source:** `docs/audits/AUDIT_FNV_2026-04-21.md`

## Problem
`parse_perk` at `crates/plugin/src/esm/records/misc.rs:524-550` reads only EDID, FULL, DESC, and the first byte of DATA. The runtime-behaviour-driving PRKE/PRKC/DATA entry-point groups are unparsed — coverage gap, not a regression. FNV ships ~120 perks; behavior dormant until perk runtime lands.

## Audit's defer rationale
> "When the perk-entry-point condition pipeline (`perk_entry_points.md`) lands, extend PerkRecord with a Vec<PerkEntry> parsed from PRKE groups."

> "Not audit-blocking for M24 Phase 1."

## SIBLING
SPEL / MGEF condition lists also parse PRKE-style blocks?

## TESTS
Regression on Bloody Mess perk (EntryPoint damage modifier).

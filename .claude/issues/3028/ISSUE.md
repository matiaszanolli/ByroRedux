# SAVE-D6-03: save-load-roundtrip.md stale in three places

**Issue**: #3028
**Severity**: LOW
**Dimension**: 6 — engine integration
**Labels**: `low,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_SAVE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAVE_2026-08-16.md` (Dimension 6 — engine integration, doc rot).

**Location**: `docs/engine/save-load-roundtrip.md`:42-47 (§2), :62-70 (§3), :141-147 (§6)

## Description

`docs/engine/save-load-roundtrip.md` — the subsystem's **authoritative cross-cutting trace**, named as such in `.claude/commands/_audit-common.md`'s reference-doc table — is stale in three places.

## Impact

This is the document an auditor or implementer is instructed to prefer over re-deriving from source. Where it is wrong, it propagates: several findings in this sweep had to be established by reading code precisely because the trace could not be trusted.

Doc rot in a named-authoritative document costs more than the same rot elsewhere.

## Suggested Fix

Refresh §2, §3 and §6 against the current code. While in the file, reconcile it with the findings filed from this sweep — in particular #3021 (`CurrentCellContext` lifecycle) and #3022 (the additive-only overlay's invalidated premise), both of which the trace describes.

## Related

- #3021 (SAVE-D6-2026-08-16-01 — the cell-context lifecycle this trace documents)
- #3022 (SAVE-D1-2026-08-16-01 — the additive-only contract this trace describes)
- #2971 (UI-D4-02) and #2975 (TD3-2026-08-16-01) — the same named-authoritative-doc rot class in this sweep

## Completeness Checks
- [ ] **ALL-THREE**: §2, §3 and §6 all corrected, not just the first
- [ ] **SIBLING**: Reconciled with #3021 and #3022, whose fixes change what the trace should say
- [ ] **PATH-GATE**: `.claude/commands/_audit-validate.sh` still passes
- [ ] **AUTHORITATIVE**: The doc is accurate enough to be preferred over source again

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3028 --json state` when live state is needed.*

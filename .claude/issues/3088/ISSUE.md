# AUD-2026-08-16-D7-01: ROADMAP M44 row stale counts + self-contradiction

**Issue**: #3088
**Severity**: LOW
**Labels**: `low,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_AUDIO_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_AUDIO_2026-08-16.md` (Dimension 7 — documentation).

**Location**: `ROADMAP.md` (the M44 row)

## Description

`ROADMAP.md`'s M44 row reports **stale test counts** and **contradicts itself on the reverb-toggle wiring**.

## Impact

`ROADMAP.md` is one of the two authoritative status documents (`_audit-common.md` names it for milestones and project stats, refreshed each `/session-close`). A row that both misreports counts and disagrees with itself gives no usable answer about M44's state.

The self-contradiction is the worse half — a stale count is merely old, but two incompatible claims in one row means a reader cannot tell which to trust.

## Suggested Fix

Re-run the audio test suite for the true count and resolve the reverb-toggle contradiction against the code (`crates/audio/src/lib.rs`'s reverb send and its `boot.rs` registration).

## Related

- #3087 (AUD-D6-01 — the same subsystem's in-code wiring comments)
- #2975 (TD3-2026-08-16-01), #2961 — the same authoritative-status-doc rot class this sweep

## Completeness Checks
- [ ] **COUNTS-MEASURED**: Test counts come from a real run, not an estimate
- [ ] **CONTRADICTION-RESOLVED**: The reverb-toggle claim is single-valued and matches the code
- [ ] **SIBLING**: Adjacent ROADMAP milestone rows spot-checked for the same drift
- [ ] **PATH-GATE**: `_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3088 --json state` when live state is needed.*

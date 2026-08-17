# AUD-2026-08-16-D6-01: stale audio scheduler-wiring comments

**Issue**: #3087
**Severity**: LOW
**Labels**: `low,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_AUDIO_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_AUDIO_2026-08-16.md` (Dimension 6 — scheduler wiring).

**Location**: `crates/audio/src/lib.rs` and `byroredux/src/boot.rs` (the registration comments)

## Description

Stale audio scheduler-wiring comments: `audio_system` is described as a **"Phase 1 stub"** and `reverb_zone_system`'s registration is attributed to **`main.rs`**.

Neither is true. `audio_system` is a live system (moved to exclusive under M27 Phase 3), and every scheduler registration lives in `byroredux/src/boot.rs` — `_audit-common.md` names `boot.rs` as *"the authority for 'which stage does X run in'"*.

## Impact

Doc rot on the subsystem's wiring description. The `main.rs` attribution is the more misleading half: it survives from before the #2731 split, so a reader looking for the registration goes to a file that no longer contains it.

Calling a live system a "Phase 1 stub" also invites a reader to conclude the audio path is unfinished when it is wired.

## Suggested Fix

Update both comments: `audio_system` is live and exclusive; registrations are in `boot.rs`. Grep the audio crate for other `main.rs` references stranded by #2731.

## Related

- #2731 (the `main.rs` split that stranded the attribution)
- #3086 (AUD-D1-01), #3088 (AUD-D7-01) — the other two audio findings
- #2971, #3028 — the same post-#2731 `main.rs` staleness elsewhere

## Completeness Checks
- [ ] **BOTH-CLAIMS**: The "Phase 1 stub" and the `main.rs` attribution both corrected
- [ ] **SIBLING**: Other `main.rs` references in `crates/audio` checked for #2731 staleness
- [ ] **BOOT-AUTHORITY**: Comments point at `boot.rs` as the registration authority
- [ ] **PATH-GATE**: `_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3087 --json state` when live state is needed.*

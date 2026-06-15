# TD8-002: Stale allow(dead_code) + comment on RefrTextureOverlay::inner — now used

_Filed as #1633 from `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`. Immutable snapshot as-filed; GitHub is authoritative for live state._

**Severity**: LOW · **Dimension**: Dead Code · **Effort**: trivial
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md` (TD8-002)
**Status**: NEW (the prior 2026-05-13 "future-tracked TD2-012" classification is now stale — the field is used)

## Description
`RefrTextureOverlay::inner` (`byroredux/src/cell_loader/refr.rs:64`) carries `#[allow(dead_code)]` and a comment "Not yet consumed by the spawn path," but `inner` is read / `&mut`-ed in the slot-index-6 XTXR lookup. Removing the allow → no warning.

## Evidence
`refr.rs:62-65` — `/// Not yet consumed by the spawn path; preserved for parity …` + `#[allow(dead_code)] pub(crate) inner: Option<FixedString>,`. Uses at `refr.rs:116` (`Self::fill(&mut self.inner, ts.inner.as_deref(), pool)`), `:150` (`6 => ts.inner.as_deref()`), `:165` (`6 => &mut self.inner`).

## Impact
Stale dead-code suppression + a contradicting "not yet consumed" comment on a field that is in fact consumed.

## Suggested Fix
Delete the `#[allow(dead_code)]` and the "not yet consumed" comment; keep the parity note describing the slot-6 round-trip.

## Completeness Checks
- [ ] **TESTS**: `cargo check -p byroredux` is warning-clean after removing the allow (field is genuinely used)

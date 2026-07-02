# 1858: TD1-003: byroredux/src/main.rs is 2846 LOC (App + event loop + world build)

https://github.com/matiaszanolli/ByroRedux/issues/1858

Labels: bug, low, tech-debt

**Severity**: LOW · **Dimension**: 1 (Complexity)
**Location**: `byroredux/src/main.rs:2228-2607` (`about_to_wait`, 379 LOC), `:110-457` (`main`, 347 LOC), `:1609-1916` (`render_one_frame`, 307 LOC), `:753-1007` (test helper `mg07_on_activate_dispatch`, 254 LOC), `:1183` (`step_streaming`, 204 LOC)
**Status**: NEW (crossed 2000 LOC since the Session-34 split; grew again after `#1670`'s `App::new` fix landed)
**Audit**: docs/audits/AUDIT_TECH_DEBT_2026-07-02.md (TD1-003)

## Description
The binary's `main.rs` is 2846 LOC, carrying the winit `ApplicationHandler` impl,
boot/config, world construction, and the streaming/transition stepping in one file.
`#1670` (CLOSED) previously split `App::new` (581 LOC) into three phase helpers — that fix
held, `App::new` is now ~117 LOC — but the file has grown back around different large
functions: `about_to_wait` (379 LOC), `main` (347 LOC), and `render_one_frame` (307 LOC).

## Evidence
fn-boundary scan: `about_to_wait` @2228, `main` @110, `render_one_frame` @1609,
`step_streaming` @1183.

## Impact
Boot config, event handling, and frame stepping are all co-located, increasing merge/review
cost on any change touching the event loop or per-frame stepping.

## Suggested Fix
Split boot/config (`main`, `build_world`) into a `boot.rs` module and keep the
`ApplicationHandler` impl in `main.rs`; the streaming/transition steppers (`step_streaming`,
`step_cell_transition`) can move to an `app_step.rs`. Effort: medium.

## Related
Distinct from the now-fixed `#1670` (TD9-NEW-04, `App::new` constructor complexity, CLOSED —
this finding targets different functions that grew after that fix landed).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix


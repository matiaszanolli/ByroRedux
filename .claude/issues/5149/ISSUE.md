# #5149: TOOL-D1-2026-09-29-06: no test pins the debug-server listener to loopback

**Labels**: low, bug, tech-debt, test-gap

**Source report**: `docs/audits/AUDIT_TOOLING_2026-09-29.md`
**Severity**: LOW
**Dimension**: Debug Trust Boundary (a guard gap, not a live defect)

## Location
`crates/debug-server/src/listener.rs` (`TcpListener::bind(("127.0.0.1", port))`)

## Description
The whole trust model rests on the literal `127.0.0.1`: there is no authentication, and the path confinement assumes a local caller. No test asserts `handle.local_addr().ip().is_loopback()`. A change to `0.0.0.0` (for example "to reach it from a VM") would pass every existing guard.

## Evidence
`grep is_loopback crates/debug-server/src` is empty; the only other `127.0.0.1` in `listener.rs` is a test that occupies a port.

## Impact
A future edit could silently expose world mutation to the network. The severity table puts a "network-reachable … without an explicit opt-in" surface at a HIGH floor, so the regression this guard would catch is HIGH.

## Related
#4752. Label gap: debug server has no own label → `tech-debt`.

## Suggested Fix
In `listener.rs` tests: `let (_, h) = spawn(0).unwrap(); assert!(h.local_addr().ip().is_loopback());`.

Validated at HEAD 9fcfdc3fc: bind literal is `127.0.0.1`; no loopback assertion exists in the crate.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix

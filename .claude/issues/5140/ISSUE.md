# #5140: TOOL-D1-2026-09-29-01: byro-dbg's new 10 s read timeout is shorter than the debug server's 30 s response timeout, so 10–30 s commands close the REPL and TUI

**Labels**: medium, bug, tech-debt

**Source report**: `docs/audits/AUDIT_TOOLING_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: Debug Trust Boundary (exposure: developers and every byro-dbg smoke harness)

## Location
- `tools/byro-dbg/src/main.rs` (`configure_read_timeout`, applied before both REPL and TUI; `Receive error` → `break`)
- `tools/byro-dbg/src/tui.rs` (net thread `Err(_) => break` → "engine disconnected", quit)
- `crates/debug-server/src/listener.rs` (`COMMAND_RESPONSE_TIMEOUT` = 30 s)

## Description
- `dd99cd0f3` (2026-09-26) raised the server's wait from 5 s to 30 s ("The previous five-second limit dropped the initial water diagnostics on FO3's exterior fixture").
- The next day, `63c0aee3b` (the #4754 fix) gave the client a fixed 10 s socket read timeout.
- Any response that arrives after 10 s but within 30 s now fails in the client with `WouldBlock`/`TimedOut`. The REPL prints `Receive error` and exits, dropping every remaining line of a piped heredoc. The engine still runs the command, then logs a write error to the closed socket.
- The TUI's net thread breaks, and the dashboard quits with "engine disconnected". The TUI can trigger this itself: a Loader-tab cell load runs synchronously in `step_debug_loads` between frames, so the next `Metrics` poll waits out the whole load.

## Evidence
`stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))` (byro-dbg) vs `const COMMAND_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);` (server). Neither side references the other.

## Impact
The same cold-stall class `dd99cd0f3` fixed is broken again on the client side (first command after bench-hold, exterior water diagnostics, cell loads). Smoke heredocs end in `|| true`, so the truncation is silent; later gates read missing output as FAIL or as a parsed `0`.

## Related
#4754 (its fix introduced this), `dd99cd0f3`, #1007, TOOL-D1-2026-09-29-02 (makes it certain while paused). Label gap: debug server / byro-dbg have no own label → `tech-debt`.

## Suggested Fix
Put one timeout constant in `debug-protocol` and set the client read timeout above the server's response timeout (e.g. 35 s). On a timeout, the TUI should report "engine busy" instead of quitting.

Validated at HEAD 9fcfdc3fc: byro-dbg `main.rs` sets a 10 s read timeout; `listener.rs` `COMMAND_RESPONSE_TIMEOUT` is 30 s.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other clients of the debug protocol, smoke-harness timeouts)
- [ ] **TESTS**: A regression test pins this specific fix

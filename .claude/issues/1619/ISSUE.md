# RT-1: audit-runtime skill endorses parallel runs that collide on debug port 9876 (no rebind)

- **GitHub**: #1619
- **Severity**: medium
- **Labels**: medium, tech-debt, bug
- **Source**: docs/audits/AUDIT_RUNTIME_2026-06-14.md (RT-1)

## Description
The debug server binds TCP 9876 exactly once at startup (`crates/debug-server/src/listener.rs:181`) and logs the bind failure (`listener.rs:184`) — it never retries or rebinds. `.claude/commands/audit-runtime/SKILL.md:137` tells the operator to run up to 4 games in parallel; the second engine's telemetry is then permanently unreachable for that run, even after the first engine is killed.

## Evidence
`/tmp/audit/runtime/fo4-InstituteBioScience.engine.log`: `Debug server failed to bind port 9876: Address already in use (os error 98)`. Parallel FNV+FO4 launch left FO4 with a dead debug server; `byro-dbg` → `Connection refused`.

## Suggested Fix
Delete the "Up to 4 games run in parallel" line and serialise; optionally per-process `BYRO_DEBUG_PORT` offset and/or listener retry-bind.

## Note
No `audit-infra` label in repo — mapped to `tech-debt`.

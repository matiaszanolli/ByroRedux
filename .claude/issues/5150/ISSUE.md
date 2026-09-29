# #5150: TOOL-D2-2026-09-29-01: `debug-cli.md` residual drift after the `63c0aee3b` reconcile — base64 screenshots, the `/tmp` tex.dump default, and the "5 s" timeout

**Labels**: low, documentation, doc-rot, tech-debt

**Source report**: `docs/audits/AUDIT_TOOLING_2026-09-29.md`
**Severity**: LOW
**Dimension**: Protocol & Registry (exposure: developers)

## Location
- `docs/engine/debug-cli.md` (`Screenshot { path? }` protocol row; `tex.dump` row; the "per-client thread's 5 s `recv_timeout`" note; "Last reconciled 2026-08-25" stamp)
- `crates/debug-protocol/src/lib.rs` (`DebugResponse::Screenshot`)
- `tools/byro-dbg/src/display.rs`, `tools/byro-dbg/src/tui.rs` (still match the dead variant)

## Description
1. `Screenshot { path? }` "…else returns base64 PNG". The server never does this: with `path: None` it writes `screenshot_<secs>.png` and answers `ScreenshotSaved`. `DebugResponse::Screenshot` is never constructed anywhere — a dead variant that byro-dbg still matches.
2. `tex.dump` "default `/tmp/tex_dump.png`". It is now `texture-dumps/tex_dump.png` and needs a startup-configured archive, which contradicts the file's own header.
3. "The per-client thread's 5 s `recv_timeout`". It is 30 s (`dd99cd0f3`), and the new 10 s client timeout (TOOL-D1-01) is not documented.
4. "Last reconciled 2026-08-25".

## Evidence
The doc lines quoted above are present at HEAD; `DebugResponse::Screenshot` has only match-arm references (byro-dbg `tui.rs`/`display.rs`), no constructor.

## Impact
Doc rot only. Readers get the wrong behaviour for two commands.

## Related
#4756 (open; its scope — counts and missing rows — is fixed; this is the residual content drift), TOOL-D1-2026-09-29-01, #4374 (removed `ListLoadedAssets` for the same dead-variant reason). Label gap: debug server / byro-dbg have no own label → `tech-debt`.

## Suggested Fix
Correct the three statements and bump the reconcile stamp. Then either implement the documented base64 return for `path: None` or delete the `Screenshot` response variant.

Validated at HEAD 9fcfdc3fc: debug-cli.md still carries the base64, `/tmp/tex_dump.png`, 5 s and 2026-08-25 lines; no `DebugResponse::Screenshot` constructor exists.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other debug-cli.md rows touched by 63c0aee3b)

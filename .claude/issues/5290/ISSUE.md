# #5290: TOOL-D2-2026-10-05-01: Absolute-path guidance outlives #4752/#5165 in `debug-cli.md` and byro-dbg, and #5150 closed without its addendum

Labels: low,tech-debt,documentation,doc-rot
Filed from: docs/audits/AUDIT_TOOLING_2026-10-05.md

**Source**: `docs/audits/AUDIT_TOOLING_2026-10-05.md` (TOOL-D2-2026-10-05-01) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: Protocol & Registry
- **Exposure**: developers using byro-dbg / the TUI loader. A request that follows the docs is silently dropped.
- **Location**:
  - `docs/engine/debug-cli.md:825-832` (LoadNif / cell-load bullets), `:17-18` (summary), `:1307-1308` (session example)
  - `tools/byro-dbg/src/tui.rs:650` (Loader hint)
  - `tools/byro-dbg/src/display.rs:340` (`.help`)
- **Status**: NEW. It is the residual of closed #5150, whose 2026-10-02 addendum comment named `:1284`, which has since shifted to `:1307`.
- **Description**: #4752 (`63c0aee3b`) and #5165 (`554259315`) made every debug-load file path relative to a startup `--esm`/`--master`/`--bsa` directory, and `write_screenshot` takes only a bare filename under `screenshots/`. Several user-facing texts still teach the old contract:
  - `debug-cli.md:825` says `LoadNif`'s `path` "is either an absolute loose-file path or an archive-relative" one.
  - The session example at `:1307` runs `screenshot /tmp/debug_frame.png` and shows `Screenshot saved: /tmp/debug_frame.png`.
  - The cell-load bullet (`:829`) and the summary (`:17-18`, "Loose `LoadNif` paths stay within configured game data roots") never state that cell loads follow the same rule. Only the protocol rustdoc (`crates/debug-protocol/src/lib.rs:107-111`) does.
  - The TUI Loader shows "Path may be loose absolute or BSA-relative." (`tui.rs:650`).
  - `.help` lists `screenshot <path>  Capture screenshot to specific file` (`display.rs:340`).

  `6f38b335d` ("Fix #5150") reconciled the screenshot *response* contract and deleted the base64 variant. It did not touch these lines, and #5150 was closed with the addendum still open.
- **Evidence**:
  - `confine_to_roots` (`byroredux/src/debug_load.rs`) returns `None` for `!requested.is_relative()`.
  - `exec_load_nif` then falls through to the archive scan, misses, and logs.
  - The request was already answered `DebugResponse::Ok` at enqueue time (`crates/debug-server/src/evaluator.rs:83-130`), so the client never sees the rejection.
- **Impact**: A developer who follows the TUI hint or the doc gets `Ok` and no load, and the reason is only in the engine log. The screenshot example returns an error. No security impact: the confinement itself is correct.
- **Related**: #5150, #4752, #5165, #5009 (same resolver, owned by `/audit-parsers`)
- **Suggested Fix**:
  - Rewrite the `LoadNif` bullet and the `:17-18` summary to state the root rule for all three load requests.
  - Change the example to `screenshot debug_frame.png` → `Screenshot saved: screenshots/debug_frame.png`.
  - Correct the TUI hint and the `.help` line.
  - Optionally, have `eval_request` reject an absolute or `..` path synchronously with an `Error`, so the client sees it.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it

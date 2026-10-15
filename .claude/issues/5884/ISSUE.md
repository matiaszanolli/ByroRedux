# #5884: PEX-D4-2026-10-15-02: AST spans are never remapped through `OffsetMap`, despite the doc

- **Labels**: low,bug,scripting
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

**Full title**: AST spans are never remapped through `OffsetMap`, despite the doc; the extender preflight reports wrong source lines after any continuation

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/lib.rs:56-110`. The doc at `:61-63` says "Spans in the returned `Script` are remapped back to original-source coordinates via the `OffsetMap`", but only error spans are remapped (`:93-96`, `:103-106`). `parse_expr` remaps only the root span (`:40-43`). The same claim is in `docs/engine/papyrus-parser.md:63-65`. The consumer is `crates/scripting/src/compatibility.rs:564-565,629-636`.
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: Every `Spanned<…>` inside the AST keeps preprocessed offsets. `analyze_source_compatibility` passes `expr.span.start` into `source_line(source_text, …)` against the original text, so each earlier `\` continuation moves the reported line back by one.
- **Evidence**: The probe parses `…x = 1 + \` ⏎ `2` ⏎ `SKSE.Foo()`. The second statement's span starts at 38, which in the original text is the `2` of the continued line (original `SKSE` is at 40), so `source_line` reports line 4 for a call on line 5. A separate scripting-side issue: `source_line` counts only `\n`, so a CR-only file reports every finding on line 1. Route that to `/audit-scripting`.
- **Impact**: Preflight diagnostics (`examples/extender_preflight.rs`) point at the wrong line. No runtime effect.
- **Related**: 01 (same fix removes the map), #2668, #3944
- **Suggested Fix**: Fix 01 with a lexer-level continuation and spans become original by construction. Otherwise add a span-remapping AST visitor in `parse_script` / `parse_expr`, or correct the doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix

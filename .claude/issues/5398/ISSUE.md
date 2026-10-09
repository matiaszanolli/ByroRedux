# #5398: PEX-D4-2026-10-08-01: Script-header `Conditional` (and `BetaOnly` / `Default`) is rejected and unrepresentable — 1,182 vanilla script objects carry it; struct-member `Hidden` and group `Collapsed` likewise rejected

**Labels**: medium,scripting,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5398

**Source**: `docs/audits/AUDIT_PAPYRUS_2026-10-08.md` — `PEX-D4-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `ScriptFlags` still has only `NATIVE|CONST|DEBUG_ONLY|HIDDEN` and the `parse_script_header` loop breaks on anything else.

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/script.rs:109-135` (`parse_script_header` flag loop),
  `crates/papyrus/src/ast.rs:47-55` (`ScriptFlags`: `NATIVE | CONST | DEBUG_ONLY | HIDDEN` only),
  `script.rs:255-273` (`parse_variable_flags`, which struct members also go through: `Conditional`
  and `Const` only), `script.rs:705-720` (group flags: `CollapsedOnRef` and `CollapsedOnBase` only)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The FO4 CK *Flag Reference* (local falloutck wiki dump) defines these flags:
  - **Script:** `Conditional`, `Const`, `DebugOnly`, `BetaOnly`, `Hidden`, `Native`, `Default`.
    `Conditional` is also the standard Skyrim script flag.
  - **Struct member:** `Hidden`.
  - **Group:** `Collapsed`, alongside `CollapsedOnRef` and `CollapsedOnBase`.

  The header loop stops at anything outside its four flags. Before #5322, the leftover token caused an
  "expected type" error. It now causes an "end of statement" error and the rest of the line is skipped,
  so either way the file records an error. `ScriptFlags` has no `CONDITIONAL` bit, so the flag cannot
  appear in the AST even after a fix to the loop. The `.pex` side (`lower.rs:505-508`) maps only
  `CONST`.
- **Evidence**:
  - Probes, identical on the old and new parser except for the error text:
    - `ScriptName DialogueFollowerScript extends Quest Conditional` → 1 error
    - `… Const Hidden Conditional` → 1 error at `Conditional`
    - `… BetaOnly` → 1 error
    - `… Default` → 1 error
    - `Group G Collapsed` → 1 error
    - struct member `Float b Hidden` → 1 error
  - Corpus census (scratch probe over the `.pex` user-flag tables, measured this pass): **1,182** script
    objects have the `conditional` user flag set. That is Skyrim SE 761 of 14,026, FO4 346 of 7,875, and
    Starfield 75 of 4,740. Each one's `.psc` source opens with a header this parser rejects.
- **Impact**: About 1,200 vanilla `.psc` files can never parse with zero errors. That includes the
  quest scripts whose variables the condition system reads (`GetVMQuestVariable`). So a strict-fail
  caller (`result.1.is_empty()`) refuses all of them. `extender_preflight` counts each one as an input
  error. Recovery skips only the header line and items still parse, so this is not silent and not a
  crash. MEDIUM, consistent with #5328.
- **Related**: #5328 (FO4+ grammar forms), #4763 (raw-stream flag sites), `script.rs:26-30` module doc
  (unknown flags become errors by design, but `Conditional` is a known keyword)
- **Suggested Fix**:
  - Add `CONDITIONAL`, `BETA_ONLY` and `DEFAULT` to `ScriptFlags`, accept them in the header loop, and lex
    a `Default` and a `Collapsed` keyword. Both need adding to `keyword_as_ident` so they stay valid
    names.
  - Let struct members take `Hidden`.
  - Map the `.pex` object user flags (`conditional`, `hidden`, `default`) into `ScriptFlags` in
    `lower.rs` so both frontends agree.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (struct-member and group flag loops; `.pex` `lower.rs` user-flag mapping)
- [ ] **TESTS**: A regression test pins this specific fix

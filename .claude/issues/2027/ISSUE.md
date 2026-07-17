# SCR-D1-NEW-01: Four of six PexError variants have zero test coverage anywhere in the repo

**Labels**: low, tech-debt, bug

**Severity**: LOW
**Dimension**: PEX Reader & Opcode Decode
**Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-07-16.md`

## Location
`crates/pex/src/reader.rs:150-161` (`value` → `BadValueType`), `:139-148` (`string_index` → `BadStringIndex`), `:463-497` (`read_instructions` → `BadOpcode`, `BadVarArgCount`)

## Description
`BadMagic`/`UnexpectedEof` are covered; the var-arg huge-positive-count path is covered (but only the success arm, not the `_ => Err(BadVarArgCount)` reject arm). A repo-wide grep finds `BadValueType`/`BadOpcode`/`BadVarArgCount`/`BadStringIndex` referenced only at their construction sites and enum definition — no test constructs `.pex` bytes that trigger any of the four. Manual review confirms all four implementations are currently correct; this is a coverage gap, not an active defect.

Verified current: grep for each of the four `PexError` variants across `crates/pex/` finds only the construction sites in `reader.rs` and the enum definition in `lib.rs` — no test invocation.

## Impact
None today. These are exactly the four decode paths a future opcode-table edit or `Value` enum change would most likely silently break, and none would be caught by `cargo test` or the corpus smoke harness (which only exercises well-formed game `.pex`, never these reject-on-malformed branches).

## Suggested Fix
Add four hand-built-`.pex` regression tests — a value-type tag of 6, an opcode byte of `MAX_OPCODE`, a string-table index one past the table length, and a var-arg count of `Value::Integer(-1)` — each asserting the specific `PexError` variant, mirroring the existing `hostile_vararg_count_errors_instead_of_ooming`/`rejects_bad_magic` pattern.

## Completeness Checks
- [ ] **SIBLING**: Mirror the existing `hostile_vararg_count_errors_instead_of_ooming` / `rejects_bad_magic` test pattern for all four variants
- [ ] **TESTS**: A regression test pins this specific fix

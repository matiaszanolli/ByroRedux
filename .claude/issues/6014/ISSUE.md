# SCR-D6-2026-10-17-04: printf flags (`+`, `-`, space, `e`, `g`) fall through as literal text and do not advance the argument index

Issue: #6014
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D6-2026-10-17-04: printf flags (`+`, `-`, space, `e`, `g`) fall through as literal text and do not advance the argument index

- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `messages.rs:203-231`
- **Status**: NEW (documented as "width ignored", but flags are not mentioned; #5789 only covered `0`)
- **Untrusted-Input**: No
- **Description**: `Template/MessageFormatting.wiki` lists the switches `+`, `<space>`, `-`, `0` and `%g`/`%.3e`. `%+-5.0f`, `% 5.2f`, `%e` stop the `spec` scan at the flag char, emit `%` + spec literally and do **not** consume an argument, so every later conversion in the same message reads the wrong value. `%05.2f` (flag `0`) parses, but width/zero-fill is ignored (`"123.45"` instead of `"00123.45"`).
- **Suggested Fix**: Parse `[+- 0#]*` flags before width; consume the argument for any recognised conversion even if the flag is not honoured; add `g`/`e`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix

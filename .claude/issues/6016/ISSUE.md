# SCR-D6-2026-10-17-06: `If`/`ElseIf` expression span is bounded by the script buffer, not the statement payload (parity gap with #5655)

Issue: #6016
Labels: low, scripting, bug, legacy-compat, game:oblivion

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D6-2026-10-17-06: `If`/`ElseIf` expression span is bounded by the script buffer, not the statement payload (parity gap with #5655)

- **Severity**: LOW
- **Dimension**: Legacy ObScript
- **Location**: `obscript_vm.rs:344-353` (`cond_end` checked only against `self.bytes.len()`)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: #5655 bounded argument reads by the statement region and `exec_set` is bounded by `end`, but the If/ElseIf arm header reads `expr_len` at `offset + 6` and evaluates `[offset+8, offset+8+expr_len)` against the whole buffer. A statement whose declared payload is shorter than `4 + expr_len` makes the condition read the following statements' bytes (`next_statement` then continues from the declared payload end), so a malformed script evaluates a wrong condition rather than reporting `Malformed`. No panic/OOB (all slices are inside the buffer); correctness-of-malformed-input only.
- **Suggested Fix**: Take `payload_span(self.bytes, offset)?` and require `cond_end <= payload_end`; add a leg to `straddling_argument_payloads_report_malformed_not_next_statement_bytes`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix

# #6147: TD2-2026-10-19-02: The SCDA statement frame (`[op][0x1c prefix][len][payload]`) is parsed three times, with the opcode constants spelled three ways
STATE: OPEN
LABELS: bug,low,tech-debt,scripting

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/obscript.rs:14-17,95-125` (diagnostic scan), `obscript_runtime.rs:28-40,331-345` (`compiled_line`), `obscript_vm.rs:69,434-450,484-640` (`next_statement`, `payload_span`, raw `0x16/0x17/0x18/0x19/0x1c` literals)
- **Age**: obscript.rs/runtime 2026-09-01; VM 2026-09-18 (6229e7d6e)
- **Effort**: small
- **Description**: three independent walkers of the same compiled-statement framing; the `0x1c` reference-function escape adds a 4-byte prefix in each (`offset + 4/6/8` literals at `obscript.rs:100-106`, `runtime.rs:333-337`, `vm.rs:435-438`). `SET_TO/IF/ELSE_IF/REFERENCE_FUNCTION` are `const`s in the first two files and bare hex in the VM (`OP_SET` at `vm.rs:69` is the only named one there).
- **Evidence**: hardening has already landed per copy: #5596 (cce3d59b5) touched all three; #5655 (291af4528, argument region bound) and #6016 (9a7298283, If/ElseIf condition bounded by payload, 2026-10-19) landed in the VM only, while `obscript_runtime.rs:327` enforces `expression_end == payload.len()` independently.
- **Proposed fix**: `pub(crate) struct StatementFrame { opcode, command_offset, payload: Range<usize>, next: usize }` + `fn statement_frame(bytes, offset) -> Option<StatementFrame>` and one `pub(crate) mod opcode { SET_TO, IF, ELSE, ELSE_IF, END_IF, REFERENCE_FUNCTION, ... }` in `obscript.rs` (the decode home); runtime and VM import both. `next_statement`/`payload_span` become thin wrappers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._


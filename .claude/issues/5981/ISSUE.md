# SCR-D2-2026-10-17-01: `Array.Remove(index, count)` overflows `index + count` and panics the engine on mod bytes

Issue: #5981
Labels: high, scripting, bug, safety

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-01: `Array.Remove(index, count)` overflows `index + count` and panics the engine on mod bytes

- **Severity**: HIGH
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/vm/natives/collections.rs:338-346` (checker accepts it at :374)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The `remove` arm computes `let end = (index + count).min(len);` on `i32` where `index` is range-checked against `len` (<= 512) but `count = int(1, 1).max(0)` is any non-negative `i32` the script supplies. `check_array_call` accepts any numeric argument for `Remove` (`:374`). `arr.Remove(1, 2147483647)` makes `index + count` overflow: in a debug/test build that is an arithmetic-overflow panic; in release (no `overflow-checks` in the workspace `[profile.release]`, `panic = "unwind"`) it wraps negative, `.min(len)` keeps the negative, and `items.drain(index as usize..end as usize)` gets an end near `usize::MAX` and panics in `Vec::drain`.
- **Evidence**: `let index = int(0, 0); let count = int(1, 1).max(0); ... let end = (index + count).min(len); items.drain(index as usize..end as usize);`. `run_fragment` is not under `catching_panics` (only `populate`/`translate_pex` are, `translate/mod.rs:143,245`), and the run happens on the main thread inside an exclusive `Update` system, so the panic takes the process down. Reachable from any VM-run quest/reference/alias script in a mod `.pex`.
- **Impact**: A single statement in a mod script crashes the game (and re-crashes on every load if the handler runs at `OnInit`/`OnCellLoad`). Same class as the already-fixed #5791 (one-statement hostile operand).
- **Related**: #5791 (caps); sibling SCR-D2-2026-10-17-02/-03.
- **Suggested Fix**: `let end = index.saturating_add(count).min(len);` (the `Value::int` operands are `i32`), and add a test beside the #5791 array tests for `Remove(1, i32::MAX)`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix

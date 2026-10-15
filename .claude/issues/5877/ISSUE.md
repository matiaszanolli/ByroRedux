# #5877: PEX-D3-2026-10-15-01: the boolean pass re-walks every if/while body it has already recursed into, so its cost is O(2^nesting depth)

- **Labels**: high,bug,scripting,performance
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

**Full title**: the boolean pass re-walks every if/while body it has already recursed into, so its cost is O(2^nesting depth). A well-formed 656-byte `.pex` takes 6.6 s and a 40-deep one would take about 30 h

- **Severity**: HIGH (the skill escalates "unbounded … recursion reachable from untrusted bytes". This is unbounded CPU behind the recursion, and the depth cap does not bound it)
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/boolean.rs:183-201` (`BoolPass::rebuild`)
- **Status**: NEW. It has been there since the port; the recursion shape is unchanged since 7fdb694bd (#1815), and the probe reproduces it on the pre-944d0299e copy too.
- **Description**: For every conditional block whose last statement computes its condition and whose true edge (`&&`) or false edge (`||`) falls through, `rebuild` first recurses over the operand range (`self.rebuild(block.on_true(), block.on_false, depth + 1)`, line 186/190). If `collapse` then declines, it continues with `it = self.cfg.next_key(current)` (line 200), which is the first block of the range it just recursed over. Every `if`/`while` header matches this shape, so each nested body is walked once by the recursion and again by the enclosing loop. With nesting depth d, the work is W(d) = 2^d − 1. `MAX_REBUILD_DEPTH` (1024) caps only the stack depth. The blow-up is already hours at d ≈ 40. `control_flow::reconstruct` avoids the problem by setting `jump_to = Some(body_end)`, which skips the body after recursing into it. The boolean pass has no equivalent. Champollion's `rebuildBooleanOperators` (PscDecompiler.cpp:851-960) has the same walk, so this is inherited.
- **Evidence**: The probe emits N nested `if a == b` with one innermost statement: `N × (cmp_eq ::temp0 a b; jmpf ::temp0 → end)`, then `assign v 1; return`. It is well-formed compiler-shaped output and decompiles `Ok`. Release build:

  | N (nesting) | file | `decompile_script` |
  |---|---|---|
  | 8 | 314 B | 49 µs |
  | 16 | 466 B | 5.4 ms |
  | 20 | 542 B | 100 ms |
  | 24 | 618 B | 1.58 s |
  | 26 | 656 B | **6.65 s** |

  The time doubles with each extra level. Per-stage timing at N=24: cfg 5 µs, lift 12 µs, **boolean 1.55 s**, reconstruct 51 µs. Without the innermost statement, the same shape fails closed in `reconstruct`, but only after the exponential boolean pass has already run (N=22: 0.27 s → `ControlFlowFailed`).
- **Orchestrator re-run** (HEAD `5f7ff5920`, release): boolean stage 3.8 ms at N=16, 15 ms at N=18, 59 ms at N=20, 244 ms at N=22, doubling per level. In this re-run the probe's final decompile returned `Err` (`ControlFlowFailed`) rather than `Ok`; the exponential time is spent in the boolean stage before either outcome, so the finding stands as stated.
- **Impact**: A mod `.pex` of under 1 KB stalls the thread that decompiles it, for minutes to effectively forever. `decompile_catching_panics` (`translate_pex`) and the fragment populate paths cannot interrupt a CPU loop, and nothing else catches it: no panic, no allocation, no error. Vanilla content is fine; the slowest vanilla decompile in the census is 15–18 ms (`cwscript.pex`, `companionaffinityeventquestscript.pex`). Hand-written or generated mod code nested about 25 deep becomes noticeably slow.
- **Related**: #1815 / #2667 (the depth cap and why it exists), and the skill's termination-guard bullet. The iterative termination argument still holds; this is a separate, time-complexity bound.
- **Suggested Fix**: After the nested `rebuild` returns and `collapse` declines (or merges nothing), continue the outer walk at the range end (`block.on_false` for `&&`, `block.on_true()` for `||`), the same way `reconstruct` does. Otherwise, mark blocks whose range has already been processed. Add a pin that decompiles a 40-deep nested `if` in under 1 s.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix

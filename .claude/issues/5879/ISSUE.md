# #5879: PEX-D3-2026-10-15-03: the short-circuit private-edge guard (944d0299e) counts in-edges by scanning the whole CFG on every fold

- **Labels**: medium,bug,scripting,performance
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

**Full title**: the short-circuit private-edge guard (944d0299e) counts in-edges by scanning the whole CFG on every fold. That is O(B) per collapse and O(B²) per function, about 47× slower than before the guard on a 65 k-instruction `||`-chain function

- **Severity**: MEDIUM
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/boolean.rs:284-297` (`incoming` closure in `BoolPass::collapse`)
- **Status**: NEW (introduced by 944d0299e)
- **Description**: `incoming(key)` iterates `self.cfg.blocks.values()` in full, and `collapse` calls it up to twice for every candidate that passes the fall-through test. A function of chained `||`/`&&` operands, with each chain kept under `MAX_EXPR_DEPTH` and many chains per function, makes about B/2 successful folds against a graph of about B blocks.
- **Evidence**: The probe (`chains` mode) has n chains of `t = a || a || … (201 operands); x = t`, so each chain is 402 instructions. Boolean-stage time:

  | n | instrs | file | boolean @HEAD | boolean, pre-guard copy |
  |---|---|---|---|---|
  | 40 | 16 k | 129 KB | 234 ms | 8.8 ms |
  | 80 | 32 k | 257 KB | 1.04 s | 27 ms |
  | 162 | 65 k | 521 KB | **4.36 s** | 92 ms |

  The `u16` instruction count bounds each function at about 4.4 s, but functions per state are also a `u16` count. The cost is therefore about 8 s per MB of file, scaling linearly in file size, against about 0.2 s per MB before the guard.
- **Impact**: A stall of the same class as -02 that hostile files can reach. Vanilla is unaffected: the slowest vanilla decompile in the census is 18 ms.
- **Related**: 944d0299e (the guard is correct; only its cost is the problem), -01
- **Suggested Fix**: Build a predecessor-count map once per pass, before `rebuild`. Keep it current inside `collapse`: drop the operand's and rejoin's out-edges, and add the adopted rejoin edges under `current`. That makes each check O(1). Alternatively, count only the blocks whose `next`/`on_false` could reach the key, using a predecessor index.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix

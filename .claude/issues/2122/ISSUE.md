# SCR-D2-NEW3-01: build_cfg attaches JmpF/JmpT condition/edges to a stale block key on a backward interior jump target

**Issue**: #2122
**Labels**: high, bug
**Dimension**: Decompiler CFG & Lift
**Untrusted-Input**: Yes — reachable from raw `.pex` bytes via `build_cfg`, on the live cell-loader VMAD-attach path (`translate_pex`). Not reachable from real Bethesda-compiler-generated `.pex`.
**Location**: `crates/pex/src/decompile/cfg.rs:213-243` (the `OpCode::JmpF | OpCode::JmpT` arm of `build_cfg`)
**Status**: NEW (found in `docs/audits/AUDIT_SCRIPTING_2026-07-21.md`, Dimension 2)

## Description

`block_key` is computed once at the top of each loop iteration (`cfg.rs:191`, `find_block_for_instruction(&blocks, ip)`), before either of this instruction's two `split_block` calls run. For `JmpF`/`JmpT`, both the fall-through split (`ip+1`) and the jump-target split (`target`) happen *before* the block's `condition`/`next`/`on_false` fields are set (`cfg.rs:224-242`).

If `target` is a **backward** offset landing strictly between the current block's `begin` and `ip` (i.e. `begin < target < ip`), the target-split subdivides the *same* physical block the first split already shrank, a second time. `block_key` — computed before either split — now points at the leftover head piece, not the tail piece that actually contains instruction `ip`. The code writes `condition`/`next`/`on_false` onto the wrong (stale) block; the block that truly ends in the conditional jump is left with `condition: None`, silently losing its `on_false` (loop-back) edge.

The sibling unconditional `OpCode::Jmp` arm does not have this bug: it sets `next` *before* performing the target-split (`cfg.rs:206`, ahead of the second split at `cfg.rs:208-211`); `CodeBlock::split` copies `self.next` into the new tail (`cfg.rs:82`), so the correct value propagates into whichever piece ends up containing `ip` even under a double-split. The `JmpF`/`JmpT` arm reorders this (both splits, *then* set fields) with no equivalent propagation trick, breaking it.

## Evidence

Reproduced by executing the actual code against a hand-built function:
```
0: assign x, y
1: assign z, w
2: jmpf t, -1        // target = 2 + (-1) = 1  (begin=0 < target=1 < ip=2)
3: return x
```
Actual `build_cfg` output:
```
block 0: begin=0 end=0 next=3 on_false=1 cond=Some("t")
block 1: begin=1 end=2 next=3 on_false=END cond=None
block 3: begin=3 end=3 next=4 on_false=END cond=None
block 4 (exit): begin=4 end=END
```
Block 0 spans only instruction 0 (unrelated to the jump) but is marked conditional on `"t"` with edges `next=3, on_false=1`. Block 1 — which actually contains the real `jmpf` at instruction 2 — is left `condition: None`, `next: 3`, with no `on_false` edge at all; the backward loop-edge to instruction 1 has vanished. Reproduction was via a temporary in-tree test, run, then fully reverted.

## Impact

A later pass (`control_flow::reconstruct`) would build an `If`/`While` gated on `t` around the *wrong* statement, while the block that really contains the loop test degenerates into unconditional straight-line code — the loop's back-edge is dropped entirely, silently changing the decompiled AST's control flow. This is a "decompiler emits wrong AST" defect per this domain's severity table (HIGH minimum), not a crash.

**Not reachable by real Bethesda-compiled `.pex`**: every existing CFG/boolean/control-flow test fixture shows `jmpf`/`jmpt` targets are always forward in real compiler output (backward is always a plain `jmp`), consistent with the 99.996% clean corpus decompile rate. This is purely a hardening gap against a hand-crafted or corrupted `.pex` (a hostile mod's VMAD script, or bit-flip corruption) reaching the live, synchronous cell-loader attach path.

## Suggested Fix

After performing both splits, don't reuse the possibly-stale `block_key` — re-resolve the block that actually contains `ip` via `find_block_for_instruction(&blocks, ip)` a second time (after both splits have settled) and write `condition`/`next`/`on_false` onto *that* key. This is robust regardless of split ordering or which split (if either) ends up subdividing the same block twice. Add a regression test (the repro above, plus a symmetric `jmpt` variant) asserting the block spanning `ip` is the one left conditional, and that no block loses its `on_false` edge when a `jmpf`/`jmpt` target is backward-and-interior to its own originating block.

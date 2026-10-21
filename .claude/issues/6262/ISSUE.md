# #6262: PEX-D3-2026-10-21-01: `lower.rs` has its own copy of `isTempVar` that disagrees with `node.rs` and with Champollion, and its doc comment says the opposite

**Labels**: low,scripting,tech-debt,bug

**Source**: docs/audits/AUDIT_PAPYRUS_2026-10-21.md · **Severity**: LOW · **Dimension**: Decompiler Boolean/Control-Flow/Lower · **Location**: `crates/pex/src/decompile/lower.rs:296-302` (the private `is_temp_var`, added by `32352592f` for #5880). Compare `crates/pex/src/decompile/node.rs:473-476`, the `pub(crate) is_temp_var` that `lift.rs:327` uses.

**Audit status**: NEW (introduced by `32352592f`)

**Untrusted-Input**: Yes (`.pex` local names), but nothing in vanilla content reaches it

## Description

The same Champollion predicate now exists twice, with different rules:

| Copy | Rule |
|---|---|
| `node.rs` (used by the lift) | `len > 6 && starts_with("::temp")` (case-sensitive) `&& !ends_with("_var")`, or `::nonevar` (caseless). This matches Champollion `PscDecompiler.cpp:20-24` exactly |
| `lower.rs` (used for local declarations) | Lowercases the name first and drops the length check. Its doc says this is "Champollion's `isTempVar` … case-insensitive, as the rest of the format is", but Champollion's `::temp` test is case-sensitive and requires length > 6 |

Champollion uses its one predicate both to decide what to declare (`:1357`) and what to emit as an assignment (`:1752`), so in Champollion the two always agree. Here they can split. A local named `::Temp1` or exactly `::temp` is treated as a **real variable** by the lift, which keeps its `::Temp1 = …` assignments. But `lower.rs` treats it as a **temporary** and does not declare it. The body then assigns and reads an undeclared name. That is the same failure #5880 just fixed for `::mangled_*`: the #5638 fragment guard declines the function, or a write-only local gets bound as a script variable.

## Evidence

The `census` probe ran both predicates over every function local in all 33,874 corpus files. They disagreed on **0** names. The corpus `::` locals are lowercase `::tempN`, `::NoneVar`/`::nonevar` and 280 `::mangled_*`. Only a non-CK compiler or hand-built bytes can produce a disagreeing name.

## Impact

None on vanilla content. On crafted or third-party-compiled `.pex` files, a function is wrongly declined or mis-bound. It is also duplicate logic, which the user's global rules prohibit, and a bookkeeping gap: the skill's "check the two still agree" holds only because of the corpus.

## Related

#5880 (closed), PEX-D3-2026-10-15-04

## Suggested Fix

Delete the `lower.rs` copy and call `super::node::is_temp_var`, which is already `pub(crate)` and Champollion-exact. That keeps one predicate for both the lift and the declaration pass.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other private copy of a Champollion predicate in crates/pex/src/decompile)
- [ ] **TESTS**: A regression test pins this specific fix

# #5880: PEX-D3-2026-10-15-04: `lower_local_decls` (0c6fe4d34) drops every `::`-prefixed local, including the compiler's `::mangled_<name>_<n>` user locals that Champollion declares

- **Labels**: medium,bug,scripting
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

**Full title**: `lower_local_decls` (0c6fe4d34) drops every `::`-prefixed local, including the compiler's `::mangled_<name>_<n>` user locals that Champollion declares. About 280 vanilla functions are left with undeclared real locals, and write-only ones are mis-bound by the VM as script variables

- **Severity**: MEDIUM (mostly a fail-closed coverage loss; a small write-only subset mis-binds silently)
- **Dimension**: Decompiler Boolean/Control-Flow/Lower
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/decompile/lower.rs:283-298` (`.filter(|local| !local.name.starts_with("::"))`)
- **Status**: NEW (an unbookkept Champollion departure introduced by 0c6fe4d34)
- **Description**: Champollion's `declareVariables` (PscDecompiler.cpp:1354-1364) skips only `isTempVar` locals, defined at PscDecompiler.cpp:20-24 as `::temp*` not ending in `_var`, or `::nonevar`. It declares `::mangled_*` locals: the compiler's renaming of a local re-declared in a sibling scope, which `getVarName` unmangles back to the source name (`:25-46`). Our filter treats any `::` prefix as a temporary, so these real user locals get no `VarDecl`. The body still assigns and reads them under the mangled name.
- **Evidence**: Census (`mangled` bin) of `::mangled_*` locals: Skyrim 31, FO4 87, FO76 12, Starfield 150. Of these, 14 are in `Fragment_*` functions: 1, 1, 0 and 12 respectively. Examples: FO4 `ms13script.DivideSpoils ::mangled_i_0` (7 uses), Skyrim `byohhousescript.BuildHousePart ::mangled_roomindex_0`. No other non-temp `::` local names occur. Downstream:
  - A read of the undeclared name fails the VM static check (`vm.rs:5067-5080`, "unknown name") and the fragment lowerer's #5638 guard, so the function declines. This is the bulk of the cases.
  - The checker's assignment arm accepts any `Ident` target unchecked (`vm.rs:4995`). A local that is only written (3 Skyrim and 3 Starfield mangled locals have ≤ 1 bytecode use, e.g. `dlc2bloodskalbladescript.OnAnimationEvent ::mangled_swordweapon_0`) therefore passes. `assign` then routes it to `write_quest_variable(owner, "::mangled_…")` (`vm.rs:1647`): a deferred write of a junk script variable on the owner.
- **Impact**: About 280 vanilla functions, including 14 fragments, decline that would otherwise lower or run. A handful of write-only cases persist a spurious variable. All four games are affected.
- **Related**: #5638, 0c6fe4d34, `user_locals_are_declared_ahead_of_the_body_and_temporaries_are_not` (it pins only `::temp0`)
- **Suggested Fix**: Use Champollion's `isTempVar` predicate (`::temp…` not ending in `_var`, or `::nonevar`, case-insensitive) as the filter, so `::mangled_*` locals are declared under the same mangled name the body uses. Add a `::mangled_i_0` case to the pin.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix

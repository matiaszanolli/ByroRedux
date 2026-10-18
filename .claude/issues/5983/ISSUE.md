# SCR-D2-2026-10-17-03: Cyclic or deeply nested struct values recurse without bound in `to_stored` (stack overflow)

Issue: #5983
Labels: high, scripting, bug, safety

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-03: Cyclic or deeply nested struct values recurse without bound in `to_stored` (stack overflow)

- **Severity**: HIGH
- **Dimension**: Fragment Dispatch & Locks
- **Location**: `crates/scripting/src/fragment/vm/value.rs:266-300` (`to_stored`), called from `interp.rs:599-603` (`write_object`) and `interp.rs:617-622` (`flush_aggregates`); assignment at `interp.rs:536-546`
- **Status**: NEW (verified by inspection; not executed - the repo is read-only for this audit)
- **Untrusted-Input**: Yes
- **Description**: Structs are `Rc<RefCell<Vec<(String, Value)>>>` shared by reference. A member assignment stores whatever value it is given (`interp.rs:541`, `members[index].1 = value;` - no type coercion; `cast` keeps objects as-is), so `s.child = s` (or `a.m = b; b.m = a`) builds a cycle. `to_stored` rejects nested *arrays* but recurses into nested *structs* with no depth limit or visited set. When a struct held in a script variable/property is written back (`flush_aggregates` at every run end, or `write_object`), the walk never terminates -> native stack overflow (SIGSEGV, uncatchable). The checker only requires `static_type(target)` to resolve (`checker.rs:489-494`), not that the value's struct type matches the member's. Without a cycle, a chain built one `x.m = y` per statement is as deep as the 100 000-statement budget allows (recursion in `to_stored`, and in `Rc` drop).
- **Evidence**: `Value::Struct(members) => { for (name, member) in members.borrow().iter() { if matches!(member, Value::Array(_)) { return None; } stored.push((name.clone(), to_stored(member)?)); } ... }`. `#5791` capped allocation but not structural depth.
- **Impact**: Mod `.pex` (FO4/Starfield struct scripts) can crash the engine with an uncatchable stack overflow at the end of the run.
- **Related**: #5791; SCR-D2-2026-10-17-01/-02.
- **Suggested Fix**: Thread a depth counter (cap ~8, structs in real content nest 1-2 deep) through `to_stored`/`from_stored` and refuse the write (`None` -> the existing "cannot be stored" stop); optionally reject a struct member assignment whose value is the container.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix

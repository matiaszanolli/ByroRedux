# #6975: TD3-2026-11-15-10: An lstring-reader doc block sits on `read_u32_sub`; the function it describes does not exist

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6975
- **Labels**: low,esm-plugin,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: 470ddd2f1 (2026-04-07).
- **Location**: `crates/plugin/src/esm/records/common.rs:303-308`
- **Description**: The doc block reads "Read a sub-record as an lstring-or-zstring. Returns `None` if absent. Collapses the `find_sub(subs, code).map(read_lstring_or_zstring)` two-liner that appears at every FULL / DESC / RNAM site. Use this instead of the open-coded pair. See TD3-007 / #1045." It is attached to `pub fn read_u32_sub` (`:309`), which reads a little-endian `u32`. No `read_lstring_sub`-style function exists anywhere in `crates/plugin/src` (`grep` is empty). So `read_u32_sub`'s rendered doc is wrong, and the contract describes a helper that was either never added or was deleted. `no_doc_block_is_spliced_onto_the_wrong_item` cannot see it, because the block's first backticked name is not a declared sibling: this is the narrow guard's documented blind spot. (Found by the Dim 2 agent and verified by the orchestrator.)
- **Fix**: Either delete the block and give `read_u32_sub` its own one-line doc, or add the helper it describes and route the FULL / DESC / RNAM sites through it. That second option is a Dim 2 consolidation; check how many open-coded pairs remain before choosing it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

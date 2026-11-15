# #6971: TD3-2026-11-15-06: `packal.md` describes `from_skyrim_procedure_tree` as the live Skyrim fallback; 88c239b35 deleted it

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6971
- **Labels**: low,gameplay,ai,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: 88c239b35 (2026-11-09, #6660) replaced it with `byroredux/src/npc_spawn/ai_package/tree.rs` ("New npc_spawn/ai_package/tree.rs replaces from_skyrim_procedure_tree").
- **Evidence**: `docs/engine/packal.md:175` ("and a `from_skyrim_procedure_tree` fallback") and `:254` ("`from_skyrim_procedure_tree` now tries `Sandbox` first, then `Patrol`"); the symbol has no definition at HEAD. Also listed in the gate's advisory.
- **Fix**: say "the procedure-tree walk (`ai_package/tree.rs`, AIP-TREE-001..008)" and, since the section is a "what shipped" history of the pre-tree stage, mark it as superseded rather than present tense.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files

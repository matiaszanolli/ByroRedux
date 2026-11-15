# #6952: TD1-2026-11-15-06: `crates/ui/src/avm2_host.rs::build_adapter_abc` is a single 375-line builder (pool construction and three method bodies in one function)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6952
- **Labels**: low,ui,game:fo4,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Age**: not measured; a crate no tech-debt report has named
- **Effort**: small
- **Labels**: `tech-debt`, `ui`
- **Location**: `crates/ui/src/avm2_host.rs:788-1163`
- **Status**: NEW
- **Description**: The function builds the three ABC constant pools (`strings`, `namespaces`, `multinames`, ~110 lines; the #2724 / #2725 derive-never-transcribe rule makes every index a returned value), then three method bodies (ready, installer, init, each a `methods` / `method_bodies` / `traits` triple), plus the optional destroy registration. The pools feed the bodies by many `Index` locals, which is why it was never split.
- **Impact**: This is the one place that assembles the bytecode adapter injected into Fallout 4 Scaleform menu movies (the module header says so). A new lifecycle hook means finding where in 375 lines to add a pool entry and a body.
- **Suggested Fix**: Two steps. First, a `struct AdapterPools { strings, namespaces, multinames, .. }` built by `fn build_adapter_pools(object, has_destroy_trait) -> (AdapterPools, AdapterNames)`. Second, `fn ready_method(..)`, `fn installer_method(..)` and `fn init_method(..)` that each append to `methods` / `method_bodies` / `traits` and return their indexes. `build_adapter_abc` becomes the ~40-line sequencer. Run the existing `avm2_host` tests to confirm the emitted ABC is unchanged (not verified here that they pin the bytes).
- **Related**: #2724, #2725, #2963.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix

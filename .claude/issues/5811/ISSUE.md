# TD2-2026-10-15-10: `populate.rs` repeats one lower-or-interpret loop three times and four `lower_*_fragment` entry points repeat the `Scope` build and `prune_unread_locals` / `locals_never_suspend` pipeline

Labels: low,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-10`).

- **Severity**: LOW (promotes to MEDIUM the first time one copy is fixed alone; M47.2 added the interpret fallback to quest, then scene + info a day later)
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/fragment/populate.rs:378-456` (quest), `:636-684` (scene), `:740-799` (info); `OwnedFragmentProviders` -> `FragmentProviderScope` conversion at `:267-285` and `:500-523`; `crates/scripting/src/translate/effects.rs:1804-1818`, `:1849-1866`, `:1871-1887`, `:2181-2206`
- **Status**: NEW
- **Age**: `e766d3147` (2026-10-13, quest fallback) -> `2362a2a2a` (2026-10-13, scene + info copy)
- **Effort**: small
- **Description**: each `*_internal` computes `quest_property_names_with_library` + `argless_function_names`, loops bindings, calls its `lower_X_fragment`, `.or_else(interpret_fallback(...))`, `attribute_provider_calls`, `is_empty` skip - 20 identical lines, differing only in the lowering call, the VM-script sink (`frags.insert_vm_script` vs `providers.vm_scripts`) and the insert. The four `lower_*` entry points differ by one `Scope` flag (`vmad_is_quest: true` on quest, a `speaker_locals` seed on info); `lower_scene_fragment` equals `lower_quest_fragment` minus that flag.
- **Evidence**: clone scan 24 windows within `populate.rs`; `effects.rs:1811-1817` vs `:1859-1865` identical.
- **Impact**: `#5407` already had to thread `own_argless_function_names` through all four; the next scope input is four signatures plus three loops.
- **Related**: #5407, #5763.
- **Suggested Fix**: `fn lower_with_scope(body, scope: Scope, providers) -> Option<Vec<Effect>>` in `translate/effects.rs` and `fn lower_or_interpret(script, fragment_name, providers, lower: impl FnOnce(&FragmentCtx)->Option<Vec<Effect>>, sink: &mut dyn FnMut(Arc<Script>)) -> Option<Vec<Effect>>` in `populate.rs`; `impl From<OwnedFragmentProviders<'_>> for FragmentProviderScope<'_>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)

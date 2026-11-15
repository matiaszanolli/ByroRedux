# #6961: TD2-2026-11-15-08: Seven near-identical `ScriptFunctionDeclaration` builders in `sdk::compatibility`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6961
- **Labels**: low,scripting,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none
- **Age**: 6c94ca3f5 / 234665577 (2026-09-05, 2026-09-23 splits; the copies are older)
- **Location**: `crates/sdk/src/compatibility/declarations.rs:12` (`papyrus_game_content_declaration`) and `:49` (`papyrus_game_entity_declaration`), `input_ui.rs:13` (`papyrus_input_declaration`) and `:48` (`papyrus_ui_declaration`), `mod_events.rs:7` (`papyrus_mod_event_declaration`), `legacy_containers.rs:21` (`papyrus_legacy_container_declaration`), `storage_util/declarations.rs:282,292,301` (`papyrus_storage_util_declaration`, `_void_declaration`, `_declaration_with_result`)
- **Description**: each builds `EnginePapyrusFunctionDeclaration { route, declaration: ScriptFunctionDeclaration { id: ScriptFunctionId::new(id).expect("built-in <X> function ID is valid"), component: ComponentId::new("<c>").expect(..), parameters: parameters.iter().cloned().map(|(id, value_type[, optional])| ScriptParameterDeclaration { id: ScriptParameterId::new(id).expect(..), value_type, optional }).collect(), result: result.map(|value_type| ScriptResultDeclaration { value_type, optional: false }), papyrus: Some(PapyrusFunctionAlias { provider, function }), description } }`. What varies is five strings (component, provider, description, and the `expect` labels) and whether the parameter tuple carries `optional` and the result is `Option`. Two of them (`declarations.rs`, `papyrus_ui_declaration`) take `(id, type)` pairs and hard-code `optional: false`, the other four take triples; the `expect` messages differ only in the product name. A fix to how an invalid ID is reported, or a new field on `ScriptFunctionDeclaration`, must be applied seven times.
- **Suggested Fix**: one `fn engine_papyrus_declaration(spec: &DeclarationSpec, route, id, function, parameters: &[(&str, ScriptValueType, bool)], result: Option<ScriptValueType>) -> EnginePapyrusFunctionDeclaration` in `compatibility/mod.rs`, with `DeclarationSpec { component, provider, description }` constants per surface. The seven become one-line wrappers or disappear into their call tables.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix

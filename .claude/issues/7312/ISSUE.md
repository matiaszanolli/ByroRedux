# #7312: SCR-D7-02: the provider once-only `OnInit` flag is stamped for every `OnInitEvent` entity, including entities with no provider program

- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Untrusted-Input**: No (growth is bounded by the number of scripted refs a plugin places)
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:27-55,155,220`
- **Status**: NEW (the #6017 fix is intact; this is a scope slip in it)
- **Description**: `initialized` is every entity carrying `OnInitEvent`. `attach_script_for_refr` (`byroredux/src/cell_loader/references/attach.rs:230`) gives `OnInitEvent` to every identity-only scripted ref, whichever script kind attached (VM or ObScript included). `first_initializations` runs on that whole set before the `PapyrusProviderProgram` query is taken. It writes `::init::papyrus_provider = 1.0` into the saved `QuestScriptVariables` under the ref's FormID for each one. For a ref with no provider program the flag is never read, so it is dead state in every save.
- **Evidence**:
  ```rust
  let initialized = world.query::<OnInitEvent>() ... .collect::<BTreeSet<_>>()   // :155
  let initialized = first_initializations(world, initialized);                     // :220
  // in first_initializations:
  variables.set(owner, key, 1.0);                                                   // :51
  ```
- **Impact**: Save-size growth, one row per identity-only scripted ref (logical actor stubs, `placement_disabled` bodies, markers) that does not use the provider seam. No functional misbehaviour was found: the VM's own `::init::<script>` flags use different keys.
- **Related**: #6017, #5997.
- **Suggested Fix**: Intersect `initialized` with the entities that hold a `PapyrusProviderProgram` (a cheap `programs.get(e).is_some()` filter, done before stamping), or stamp only inside the `enqueue(OnInit)` branch.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix

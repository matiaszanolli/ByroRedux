# #7292: SCR-D5-01: ObScript `SetPackDuration` calls the explicit-`EvaluatePackage` entry point, so since #7243 it releases a Must Complete hold and drops a running script package, and it never applies the duration

- **Severity**: MEDIUM
- **Dimension**: Scene/Package/Cinematic (the package seam; the command itself is Dim 6)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/obscript_more/state.rs:373-382` (`cmd_1122`, table row `state.rs:772`); the semantics change is in `crates/scripting/src/package.rs:252-258` (`request_package_evaluation`)
- **Status**: NEW (adjacent: D6-04 `forms` is a write-only sink behind "kept" commands, which is the generic store-and-never-read class; D3-01 is the marker-ordering bug. Neither mentions this command.)
- **Description**: #7243 made `request_package_evaluation` the *explicit-EVP* path. After `drop_script_package(world, actor, true)` it raises `ReleasePackageHold`, so the next ambient selection ignores the running package's Must Complete extension. Its doc says the internal re-evaluations that must keep the hold go through `queue_package_evaluation`. `cmd_1122` is ObScript `SetPackDuration`. Its comment says the value is "kept; the ambient AI re-evaluates the actor when it elapses". Yet it calls `request_package_evaluation` at once, before storing the number. The CS page (`cs-uesp-wiki/(main)/SetPackDuration.wiki`) says it "Sets the duration of the actor's current package to iDuration (in seconds of gametime)". It describes no re-evaluation and no cancelling.
  - Both side effects are unrelated to a duration change. A running script package (`AddScriptPackage` then `SetPackDuration`) is dropped by `drop_script_package(.., only_running = true)`. A Must Complete ambient package with no Done state loses its hold.
  - The stored value (`ObScriptLedger::set_value(0x1122, ..)`) has no reader (`rg 0x1122 crates byroredux` finds only the table row and the id census in `obscript_more/tests.rs`). So the package is never shortened.
  - The unit comment says minutes and the CS page says seconds, so even the stored number is mislabelled.
- **Evidence**:
  ```rust
  // state.rs:373-382
  // CS *SetPackDuration <minutes>*: the current package's duration — kept; the ambient AI re-evaluates
  // the actor when it elapses.
  fn cmd_1122(world: &World, _fallout: bool, cmd: u16, args: &[ObScriptValue], ctx: &Context) -> Action {
      match caller(world, ctx) {
          Some((form, actor)) => {
              crate::package::request_package_evaluation(world, actor);
              store(world, cmd, form, num(args, 0))
  ```
  ```rust
  // package.rs:252-258
  pub fn request_package_evaluation(world: &World, actor: EntityId) -> bool {
      crate::script_packages::drop_script_package(world, actor, true);
      if let Some(mut releases) = world.query_mut::<ReleasePackageHold>() { releases.insert(actor, ReleasePackageHold); }
      queue_package_evaluation(world, actor)
  ```
  The other `request_package_evaluation` callers are the explicit spellings: `Effect::EvaluatePackage` (`fragment/effects/ai_combat.rs:184`), `RefCollectionAlias.EvaluateAll` (`vm/natives/collections.rs:170`, "Calls EvaluatePackage on all the actors"), ObScript `EVP` (`obscript_actions.rs:139`) and `ResetAI` (`obscript_effects/combat.rs:171`, whose GECK text says packages are "re-evaluated"). Only `SetPackDuration` is a non-EVP caller.
- **Impact**: An Oblivion/FO3/FNV script that shortens its actor's current package ends a running script package, or a held Must Complete package, instead. The command reports Done, so no decline records the gap. The tick that follows is the very package change the script did not ask for.
- **Related**: #7243, #7224, D6-04, D3-01
- **Suggested Fix**: Either decline `SetPackDuration` (the value has no consumer) or implement the duration on the actor's running package. In both cases drop the `request_package_evaluation` call. If an immediate re-evaluation is wanted, use `queue_package_evaluation`, which keeps the hold.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix

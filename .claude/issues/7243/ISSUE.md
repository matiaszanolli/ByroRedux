# #7243: INC-2026-11-22-01: The #7224 docs say `EvaluatePackage` ends a Must Complete hold, but the ambient pass never lets it

**Labels**: medium,gameplay,ai,bug,game:fo3,game:fnv

From `docs/audits/AUDIT_INCREMENTAL_2026-11-22.md` (report ID **IA-C-01**), audited over `2b40f76b6..f9f65373a`.

- **Severity**: MEDIUM
- **Dimension**: gameplay Dim 5 (package flags)
- **Location**: `byroredux/src/npc_spawn/package_flags.rs:9-12` (module doc), `:51-60` (`procedure_done` comment, `_ => false`); `byroredux/src/npc_spawn/ai_package.rs:1359-1374` (`requested`), `:1456-1458` (`select_active_package(..., runtime.active_package_form_id, ...)`)
- **Changed in**: `package_flags.rs` / commit 94e785c7e (#7224, comment + test only)
- **Status**: NEW (the premise of closed #7224 was kept by the owner; this is the second half it left behind)
- **Description**: #7224 keeps a Must Complete package whose procedure has no Done state (Sleep, Eat, Wander, Sandbox, Guard) holding past its window "until a higher-priority package, `EvaluatePackage`, or a script package ends it, as the GECK describes". The GECK page for EvaluatePackage says the opposite of what the code does: "If the actor is on a package flagged as must complete or must reach location, EVP will still drop that package and force a reevaluation of the package list" (`/mnt/data/src/reference/geckwiki/(main)/EvaluatePackage.wiki`, Notes). In the engine, `EvaluatePackageRequest` only adds the actor to `due` so the pass runs now; the winner is still `select_active_package(.., current = runtime.active_package_form_id, ..)`, whose `kept_past_window` returns true for the running Must Complete package. So EVP does not end an ambient hold. Only the script-package slot is dropped (`request_package_evaluation` -> `drop_script_package`).
- **Evidence**: `requested.contains(&actor)` is read once, at `ai_package.rs:1371`, to widen `due`; nothing passes it to `select_active_package` / `scan_with_flags` / `kept_past_window`. The new test `a_must_complete_package_with_no_done_state_holds_past_its_window` has no EVP case, so the comment's claim is unpinned.
- **Impact**: After the #7224 decision, the only GECK-documented release from a no-Done Must Complete package (a quest script or console `evp` waking a Sleep/Eat actor) does nothing; the actor stays pinned until a higher-priority package opens. The comment tells the next reader the opposite, citing the GECK as authority. FNV `VSpecialViperShadow*` Sleep/Eat packages named in #7224 are the exposed shape.
- **Related**: #7224 (closed), #6656, `crates/scripting/src/package.rs:227-247`
- **Suggested Fix**: Make an explicit EVP (not `queue_package_evaluation`, which sets up a script package and must not end it) clear the running-package hold for that pass, e.g. pass `current = None` or skip `kept_past_window` when the request came from `request_package_evaluation`; add a test beside the #7224 one. Otherwise correct the comments to say EVP does not release it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

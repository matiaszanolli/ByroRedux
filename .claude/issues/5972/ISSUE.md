# SCR-D1-2026-10-17-04: `SetEssential` / `SetProtected` claim FO4/SF per-instance `Actor` receivers as ActorBase writes

Issue: #5972
Labels: medium, scripting, bug, gameplay

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D1-2026-10-17-04: `SetEssential` / `SetProtected` claim FO4/SF per-instance `Actor` receivers as ActorBase writes

- **Severity**: MEDIUM
- **Dimension**: Recognizer Chain
- **Location**: `crates/scripting/src/translate/effects/prims/actor_state.rs:144-170`; applier `fragment/effects.rs:402-424`; override store `bleedout.rs:65-110`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `prim_set_base_protection` matches the method names `SetEssential` / `SetProtected` on any receiver. If the receiver is not `<x>.GetActorBase()`, it treats the expression as an `ActorBase` property (`ActorBaseRef::Form(receiver_object(..))`) - the lowerer has no property-type table to tell an `ActorBase Property` from an `Actor Property`. Fallout 4 and Starfield declare `SetEssential(Bool)` / `SetProtected(Bool)` on `Actor` itself with per-instance semantics ("This works on an individual actor, not the Actor Base"), and on `RefCollectionAlias`.
- **Evidence**: FO4 `actor.pex` (dumped): `Function SetProtected(Bool abProtected)`, `Function SetEssential(Bool abEssential)`; FO4 CK wiki `SetEssential - Actor` / `SetProtected - Actor` ("individual actor, not the Actor Base"); `actorbase.pex` and `refcollectionalias.pex` also declare them. Applier: `ActorBaseRef::Form(form)` -> `resolve_property_form_id(vmad, form.property_name())`; for `Alias.GetActorRef().SetEssential(x)` the property is alias-bound so it resolves to `None` and the effect is logged and skipped; for an `Actor Property X; X.SetEssential(x)` it resolves to the reference's form id and `base_protection` keys the override by that id, which `bleedout::base_protection` (keyed by `SceneAliasCandidate::base_form_id`) never matches. Either way the call is silently inert while the fragment is reported as fully lowered, so the VM fallback never gets the statement.
- **Impact**: FO4/SF fragments that flip a companion's essential/protected state lower "successfully" with no effect (or a wrong-keyed write), losing a gameplay effect instead of declining to the VM; the per-instance vs per-base distinction is also collapsed.
- **Related**: #4415 (base protection model), M47.2 `dd843f9e6`.
- **Suggested Fix**: Lower only the `<actor>.GetActorBase().SetEssential/SetProtected(..)` shape (drop the bare-property branch, or gate it on a known `ActorBase`-typed property), and decline other receivers so the VM (which knows declared types) handles the per-instance form.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix

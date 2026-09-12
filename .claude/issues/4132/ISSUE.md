### PHYS-D1-2026-09-11-01: Dead `CollisionShape::scaled()` carries a doc comment that contradicts the corrected scale contract and would reintroduce a fixed bug class if followed

- **Severity**: MEDIUM
- **Dimension**: Shape Translation
- **Location**: `crates/core/src/ecs/components/collision.rs:51-113`
- **Status**: NEW
- **Source**: `docs/audits/AUDIT_PHYSICS_2026-09-11.md`

**Description**: `CollisionShape::scaled()` was added in `264f44fd` (2026-08-14, fixing #2868) as the ragdoll producer's scale-application mechanism. `b8c4e6af` (2026-08-17) removed that call site — `activate_ragdoll` now defers scaling entirely to the shared PHYSAL converter, with an in-code comment explaining why ("pre-scaling here produced scale² limb geometry while the joint pivots below remained scale¹"). That removal made `.scaled()` **dead code**: the only remaining non-test call sites are its own recursive call for `Compound` children (reachable only from itself) and an unrelated `RadiantIntensity::scaled` in `render/lights.rs`. Its doc comment was never updated to reflect that its own producer stopped calling it, or that the engine-wide contract flipped to "no producer pre-bakes scale; the shared converter applies it exactly once" — the exact contract `7c8347f0` just had to state explicitly in `convert.rs` and `physics.md` because the *old, wrong* version of that contract (which this method's doc still embodies) is what let the sibling `#3959` bug survive four review passes.

**Evidence**: `crates/core/src/ecs/components/collision.rs:58-60` — *"any site that turns an authored shape into a collider for a scaled instance must pass it through here first, or the collider silently keeps bind-scale proportions (#2868)"* — directly contradicts `crates/physics/src/convert.rs:139-145`'s current, corrected contract ("producers keep the shape in local units; this function applies `GlobalTransform::scale` exactly once. Baking it on both sides yields scale² geometry ... and #3959, which was exactly that"). `git log -S"pub fn scaled"` shows it added `264f44fd`; `git log -S"retain canonical authored geometry" -- byroredux/src/ragdoll.rs` shows the call site removed `b8c4e6af` — both predate this audit by 3-4 weeks and survived five prior physics audit passes (08-24, 08-29, 09-05, 09-06, 09-10). `grep -rn "\.scaled(" --include="*.rs" .` confirms no production caller anywhere in `byroredux/src`, `crates/nif/src`, or `crates/physics/src` — re-verified directly during publish: the only non-test hits are the recursive `Compound` call inside `collision.rs` itself and the unrelated `RadiantIntensity::scaled` in `render/lights.rs`.

**Impact**: currently zero — unreachable code, no live bug. The risk is entirely prospective: this is the exact documentation shape that already produced #2868 and independently #3064/#3065/#3959 (three separate producers hit the identical scale² mistake before `7c8347f0` closed the third). A doc comment instructing the *opposite* of the now-canonical contract, on a public method in `core` any producer can call, is a standing invitation to regress a bug class that has already cost four fix passes.

**Related**: same bug family as CONFIRMED-FIXED PHYS-D1-2026-09-06-01 (#3959), and its siblings #2868, #3064, #3065.

**Suggested Fix**: either delete `CollisionShape::scaled()` (and its tests) now that it has no caller — `convert.rs`'s own comment already argues against "a list of producers maintained on the consumer" style duplication, and this method is exactly that kind of duplicate scale-application surface — or, if a future direct-Rapier producer is expected to bypass the shared converter, rewrite the doc to state plainly that it must never be combined with `collision_shape_to_parts`/`ragdoll::build_ragdoll`.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other `core`-crate component carries a similarly stale "apply this yourself" doc comment left over from a pre-`7c8347f0` producer shape
- [ ] **TESTS**: If `scaled()` is deleted, remove its now-orphaned unit tests rather than leaving them to assert dead behavior; if kept, add a doc-test/comment making the "never combine with `collision_shape_to_parts`" rule unmissable

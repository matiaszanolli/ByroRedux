# #1324 -- TD-D2: Dead code in debug-ui/sfmaterial/scripting

_Snapshot as filed 2026-05-29 from /audit-publish (AUDIT_TECH_DEBT_2026-05-28)._

**Severity**: LOW | **Dim 2** — Dead Code
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-05-28.md` (D2-NEW-01, D2-NEW-02, D2-NEW-03 bundled)
**Domain**: renderer | **Effort**: trivial

**Findings**:

**D2-NEW-01** — `EguiPassConfig` declared in `crates/debug-ui/src/lib.rs` but never constructed or referenced outside the crate. The struct and any associated methods can be deleted (or the crate should expose a constructor that actually uses it).

**D2-NEW-02** — `State::class_by_type_id` (HashMap) built in `crates/scripting/src/condition.rs` but never queried. Dead HashMap allocation on every condition-evaluator construction.

**D2-NEW-03** — `crates/sfmaterial/src/lib.rs` and `crates/debug-ui/src/lib.rs` re-export several `pub` functions that have no callers in the workspace. Review and reduce the public surface to what's actually consumed.

**Fix**: Delete each unused item or mark `pub(crate)` where the visibility was accidentally widened.

## Completeness Checks
- [ ] **SIBLING**: run `cargo +nightly rustc -- -W unused` on the affected crates after deletion
- [ ] **TESTS**: no test changes needed; `cargo check` confirms
- [ ] **CANONICAL-BOUNDARY**: N/A
- [ ] **UNSAFE**: no unsafe

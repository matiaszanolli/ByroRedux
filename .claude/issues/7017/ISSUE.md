# #7017 — PHYS-D3-02: a3a36f1c7 left four statements of the admission rule stale — the rule is now three mappings and eight more rigs

**Labels**: low,physics,documentation,doc-rot,game:fo4 · **Filed from**: `docs/audits/AUDIT_PHYSICS_2026-11-15.md` (2026-11-15)

- **Severity**: LOW
- **Dimension**: Dim 3 — doc rot
- **Location**: `docs/engine/physal.md:233-241`, `docs/engine/physics.md:312-318`, `byroredux/src/ragdoll_pose.rs:194-199` (module "Scope"), `docs/smoke-tests/fo4-ragdoll.sh:17-19` and `:225-226`
- **Status**: NEW
- **Trigger Conditions**: documentation only
- **Description**: a53173780 rewrote physal.md §3 to name the per-rig maps ("Skyrim's humanoid and creature sets and the FO3/FNV humanoid map entry k to body k; FO4's human, ghoul and super-mutant sets map by `pose_bone_id`"). Six days later a3a36f1c7 added a third mapping (skeleton node order) and eight rigs, touching four files but not these:
  - `physal.md:233-241` still lists two maps, and "a rig joins by adding its *verified* body set" now overstates (three of eight were admitted on the owner's call, not the verified bar).
  - `physics.md:312-318` still says pose interpretation "is pinned to the humanoid skeleton's named body order".
  - The `ragdoll_pose.rs` module **Scope** paragraph (`:196-199`) lists "the FO4 human, feral-ghoul and super-mutant rigs" only, contradicting the FO4 section ~120 lines above it in the same doc; `:194` ("Creatures stay refused until each rig is verified") sits in the FO3/FNV section and reads as a global statement.
  - `fo4-ragdoll.sh:17-19` and `:225` still describe the molerat as "a creature whose rig is not posable"; since a3a36f1c7 it is a posed corpse (the census pins Molerat 12 of 12 resolving), so the gate's comment misdescribes what it now covers.
- **Evidence**: `git show --stat a3a36f1c7` touches `ragdoll_pose.rs`, `ragdoll_pose_tests.rs`, `workspace_hygiene_tests.rs`, `skeleton_tree_dump.rs` only; `git log -3 -- docs/engine/physal.md` last entry is a53173780.
- **Impact**: A reader (or the next audit) takes the FO4 rule as bone-id-only and the creature rigs as refused; the smoke gate's pass is read as "creature collapse" when it is a posed-corpse stability check.
- **Related**: #6065, PHYS-D3-01
- **Suggested Fix**: One sentence each: name the node-order map and the eight sets in physal.md §3 and physics.md, mark the molerat/dog/stingwing exception, update the module Scope paragraph, and reword the smoke script's molerat comment.

---

Source: `docs/audits/AUDIT_PHYSICS_2026-11-15.md`

## Completeness Checks
- [ ] **SIBLING**: Every other statement of the same rule (docs, module docs, skill text) checked
- [ ] **TESTS**: n/a (documentation) — or the doc's claim is pinned where a guard already exists

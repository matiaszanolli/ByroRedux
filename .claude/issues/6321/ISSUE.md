# #6321: LC-2026-10-21-D1-01: Two new hand-rolled `(x, z, −y)` swaps bypass `zup_to_yup_pos`

**Labels**: low,bug,legacy-compat,tech-debt

**Source**: docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md · **Severity**: LOW · **Dimension**: 1 — One implementation · **Location**: `byroredux/src/cell_loader/tree_lod.rs:263-264` (`a9384f9c0`, 2026-10-14); `byroredux/src/systems/cinematic.rs:524-526` (`696da4f47`, 2026-10-13, `ApplyHavokImpulse`)

- **Severity**: LOW
- **Dimension**: 1 — One implementation
- **Location**: `byroredux/src/cell_loader/tree_lod.rs:263-264` (`a9384f9c0`, 2026-10-14); `byroredux/src/systems/cinematic.rs:524-526` (`696da4f47`, 2026-10-13, `ApplyHavokImpulse`)
- **Status**: NEW
- **Description**: The Z-up → Y-up swap has one home, `byroredux_core::math::coord::zup_to_yup_pos`. That is the #1044 consolidation, and it is why the #333-class fixes reach every site. Two commits in range write the swap inline instead:
  - `let base = Vec3::new(x, z, -y);` places the tree-LOD billboard.
  - `Vec3::new(x, z, -y) * factor` builds a scripted Havok impulse.
- **Evidence**: An identifier-level grep for the swap outside `coord.rs` and tests finds only these two production sites. Every other hit is a comment or test assertion, and `precombined.rs:1805,1819` (2026-08-10) sits inside a bound-validation helper.
- **Impact**: Both are correct today. The risk is drift: a future change to the canonical conversion (unit scale, sanitising, a basis change) would miss them, which is the failure mode the skill was written to prevent.
- **Related**: #1044, #1112. The hand-rolled `(cos, 0, −sin)` plan directions beside the tree swap carry a yaw sign whose source is not cited; that question belongs to `/audit-exterior`.
- **Suggested Fix**: Route both through `zup_to_yup_pos`. The tree site is `Vec3::from(zup_to_yup_pos(tree.position))`; the impulse site is `Vec3::from(zup_to_yup_pos(request.vector)) * factor`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._

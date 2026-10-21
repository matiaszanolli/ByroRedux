# #6303: NIF-D5-2026-10-21-02: `extract_np_ragdoll` re-implements NIF world-transform composition (`node_world`) beside `compose_transforms`, and walks parents through a bare `NiNode` downcast instead of `as_ni_node`

**Labels**: low,bug,nif-parser,nif,physics,tech-debt,game:fo4

**Source**: docs/audits/AUDIT_NIF_2026-10-21.md · **Severity**: LOW · **Dimension**: Collision/Shader Parsing (ragdoll import) · **Location**: `crates/nif/src/import/collision/ragdoll.rs` (`node_parents`, `node_world`)

- **Game Affected**: FO4 (`bhkRagdollSystem` path only)
- **Location**: `crates/nif/src/import/collision/ragdoll.rs:531-579` (`node_parents`, `node_world`); compare `crates/nif/src/import/transform.rs:25` (`compose_transforms`) and `crates/nif/src/import/walk/mod.rs:102` (`as_ni_node`)
- **Status**: NEW
- **Description**: `node_world` builds a node's rest world transform by hand-composing the `NiTransform` chain (row-to-column `Mat3`, `t += r * (local.t * s)`, `r *= lr`, `s *= local.scale`). This is the same math as `compose_transforms`, but without the #4549 overflow sanitiser. `body_offset` rejects non-finite results afterwards, so nothing unsafe gets through. Both `node_parents` and `node_world` recognise a parent only through `downcast_ref::<NiNode>()`. A bone under any NiNode subclass with its own struct would stop the chain early: `BsMultiBoundNode`, `NiSwitchNode`, `BsOrderedNode`, `BsValueNode`, `NiBillboardNode`, `BsRangeNode`, `BsWeakReferenceNode`, `BsFaceGenNiNode`. A truncated chain gives a wrong rest world transform, and the body is then posed off its bone with an `is_t` offset computed from that wrong transform.
- **Evidence**: The `census` probe found 0 non-`NiNode` node structs in all 75 FO4 skeletons that carry a `bhkRagdollSystem`, so vanilla is unaffected. The `as_ni_node` doc (`walk/mod.rs:91-101`) exists to stop exactly this hand-rolled downcast ("Walkers should call this instead of hand-rolling the downcast chain").
- **Impact**: None on vanilla content. The cost is a second copy of transform composition (the global "improve existing code rather than duplicating logic" rule) and a latent mis-pose on modded skeletons.
- **Related**: #5015, #4549, #142/#148.
- **Suggested Fix**: Compose with `compose_transforms` over the parent chain and convert once to `Mat3` at the end. Build the parent map with `as_ni_node` (widen it to `pub(crate)`) and also handle the `switch_active_children` types.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

# #6235: ECS-2026-10-21-04: the stack root-motion mix ignores the per-bone layer mask

**Labels**: medium,ecs,animation,bug

**Source**: docs/audits/AUDIT_ECS_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 9 Animation Runtime · **Location**: `byroredux/src/systems/animation.rs:912-931`

- **Location**: `byroredux/src/systems/animation.rs:912-931` (accumulation-root pick, `let ew = layer.effective_weight()` at :915), `:934-953` (weighted delta mix, `let weight = layer.effective_weight()` at :938). Compare `crates/core/src/animation/stack.rs:396-409` (`bone_layer_weight`).
- **Status**: NEW. Masks arrived in `49529db7c` (behavior graph: layered blends). The mixed root-motion path predates them and was not updated.
- **Description**: Since `49529db7c`, every weight site in `sample_blended_transform` goes through `bone_layer_weight`, which multiplies `effective_weight()` by `clip.weight` (#469) and by the layer's mask for that bone. The root-motion path still uses plain `effective_weight()`, in two places: when it picks the accumulation root, and when it averages each layer's `sampled_root_motion_delta`. A layer whose mask gives the root bone 0 (the upper-body half of a vanilla layered blend) still contributes its root delta at full layer weight. A low-`clip.weight` layer contributes as if it had weight 1.
- **Evidence**: Masks are live in production. `crates/scripting/src/animation_graph.rs:1317` sets `stack.bone_index`, and `:1361` / `:1419` set `layer.bone_weights` from the hkbBlenderGenerator children's `boneWeights` (`crates/hkx/src/behavior.rs:1310-1318`). `e8026e737` makes graph-driven actors consume the reference-frame root motion that this mix produces.
- **Impact**: For a graph-driven actor (Skyrim/FO4 behavior graph) in a layered blend whose upper-body clip carries an accumulation-root channel, the root delta is averaged with the masked-out layer's (usually stationary) root motion. The actor then translates slower than its locomotion clip, so feet slide. The bone pose is correct, which is why the tests pass: `bone_weights_mask_a_layer_per_bone` checks only `sample_blended_transform`, and `animation_stack_root_motion_is_the_weighted_mix_of_its_layers` uses unmasked layers.
- **Related**: #3703 / #469; the skill's "Root motion" and "Layer masks" bullets; NIFAL-D7-2026-10-21-01 (a different defect in the same FO4 graph root-motion chain: the clip conversion drops the reference-frame yaw).
- **Suggested Fix**: Expose `bone_layer_weight` from `stack.rs` (or a `layer_weight_for(stack, layer, clip, name)` wrapper) and use it at :915 and :938 with the accumulation-root name. Then add a masked case to the root-motion mix test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

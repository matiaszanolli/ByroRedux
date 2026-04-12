# Investigation: #226 — NIF link resolution has no upfront validation pass

## Domain
nif

## Current state
- `NifScene::get_as<T>(index)` silently returns `None` for out-of-range or
  type-mismatched indices. Consumers (importer, cell loader) just see the
  `None` and skip the reference — a stale bug in a block parser producing
  garbage indices is indistinguishable from a legitimately null ref.
- No `validate_refs()` entry point exists.
- There is no reflection facility to discover *every* `BlockRef` field in
  every block type, but the three upcast traits (`HasObjectNET`,
  `HasAVObject`, `HasShaderRefs`) already expose the bulk of the
  per-block link surface:
  - `HasObjectNET::extra_data_refs`, `controller_ref`
  - `HasAVObject::properties`, `collision_ref`
  - `HasShaderRefs::shader_property_ref`, `alpha_property_ref`
- `NiNode::children` and `NiNode::effects` carry the scene graph edges
  and are not exposed through any trait — we downcast explicitly.

## Fix design
Add `NifScene::validate_refs()` that returns `Vec<RefError>` (empty = ok).
Walks every block and checks each non-null `BlockRef` resolves to an
in-range block index. Sources of refs:
1. The three upcast traits (covers most blocks).
2. Explicit `NiNode` downcast for children/effects.
3. `scene.root_index` itself.

Each entry carries `block_index`, `block_type`, `ref_kind`, `bad_index`
so callers can log or assert rich diagnostics.

Scope intentionally omits exhaustive per-field type checking — a LOW
severity bug-catching net, not a full schema validator. Extending the
sources later is additive.

## Files touched
- `crates/nif/src/scene.rs` — new `RefError` struct + `validate_refs()` method + tests.

Single-file fix, well under scope ceiling.

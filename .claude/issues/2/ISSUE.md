# Issue #2: Scene graph hierarchy needed for node-level animation

## Metadata
- **Type**: enhancement
- **Severity**: high
- **Labels**: enhancement, animation, ecs, import-pipeline, M21
- **State**: OPEN
- **Created**: 2026-04-02
- **Milestone**: M21 follow-up (prerequisite for M29)
- **Affected Areas**: NIF import, ECS architecture, animation system, renderer

## Problem Statement
NIF import flattens the scene graph — only leaf geometry becomes entities. Animations target NiNodes by name, but intermediate NiNodes have no entities. A parent node's animated transform should propagate to all children.

## Design Decision Needed
- `LocalTransform` + computed `WorldTransform` (two components) — standard in modern engines
- Or single `Transform` as local, renderer composes via parent chain

## Affected Files
- `crates/nif/src/import.rs` — emit entities for NiNodes
- `crates/core/src/ecs/components/` — new Parent, Children, LocalTransform, WorldTransform
- `byroredux/src/main.rs` — animation writes LocalTransform, new propagation system
- Renderer — use WorldTransform for model matrix

## Acceptance Criteria
- [ ] NiNode entities created during import with Name + LocalTransform
- [ ] Parent/Children hierarchy components
- [ ] Transform propagation system (local→world)
- [ ] Animating parent NiNode moves all child geometry
- [ ] Existing static rendering unaffected

## Depends On
Nothing — this is a foundational change.

## Blocks
- #9 (name collision scoping)
- M29 (skeletal animation)

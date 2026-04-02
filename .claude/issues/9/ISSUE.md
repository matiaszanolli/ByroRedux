# Issue #9: Name collision when multiple meshes share the same node name

## Metadata
- **Type**: bug
- **Severity**: medium
- **Labels**: bug, animation, ecs
- **State**: OPEN
- **Created**: 2026-04-02
- **Milestone**: M21 follow-up
- **Affected Areas**: Animation system entity targeting

## Problem Statement
`animation_system` builds a global `HashMap<String, EntityId>`. Multiple entities with the same name (common: "Scene Root", same NIF loaded twice) causes only the last-inserted to be targeted. Animation hits wrong entity nondeterministically.

## Affected Files
- `byroredux/src/main.rs` — animation_system name lookup

## Acceptance Criteria
- [ ] Multiple instances of same NIF animate independently
- [ ] Name collision doesn't target wrong entity

## Depends On
- #2 (scene graph hierarchy for subtree-scoped lookup)

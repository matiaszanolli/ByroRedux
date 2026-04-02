# Issue #5: BSA-based KF loading

## Metadata
- **Type**: enhancement
- **Severity**: medium
- **Labels**: enhancement, animation, import-pipeline
- **State**: OPEN
- **Created**: 2026-04-02
- **Milestone**: M21 follow-up
- **Affected Areas**: CLI, BSA extraction, animation loading

## Problem Statement
`--kf` only loads loose files. Game animations are packed in BSA archives. Need to extract from loaded BSAs, falling back to loose files.

## Affected Files
- `byroredux/src/main.rs` — KF loading section in `setup_scene()`

## Acceptance Criteria
- [ ] `--kf <path>` extracts from loaded BSA archives if found
- [ ] Falls back to loose file
- [ ] Works alongside --bsa and --textures-bsa

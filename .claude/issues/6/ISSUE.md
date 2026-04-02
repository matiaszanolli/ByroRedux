# Issue #6: NiControllerManager sequence management

## Metadata
- **Type**: enhancement
- **Severity**: medium
- **Labels**: enhancement, animation, nif-parser
- **State**: OPEN
- **Created**: 2026-04-02
- **Milestone**: Future (post-M21)
- **Affected Areas**: NIF import, animation system

## Problem Statement
NiControllerManager is the top-level animation controller in .nif files (not .kf). Manages multiple NiControllerSequence refs, handles transitions, cumulative mode, and object palette. Block is parsed but manager logic not implemented.

## Affected Files
- `crates/nif/src/anim.rs` — extend import to check for NiControllerManager
- `crates/nif/src/blocks/controller.rs` — already parsed
- Need NiDefaultAVObjectPalette block parser

## Acceptance Criteria
- [ ] .nif files with embedded NiControllerManager auto-discover sequences
- [ ] Object palette maps node names correctly
- [ ] Cumulative mode respected

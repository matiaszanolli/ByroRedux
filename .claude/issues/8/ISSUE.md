# Issue #8: Accumulation root for root motion extraction

## Metadata
- **Type**: enhancement
- **Severity**: medium
- **Labels**: enhancement, animation
- **State**: OPEN
- **Created**: 2026-04-02
- **Milestone**: Future (requires physics/character controller)
- **Affected Areas**: Animation system, character movement

## Problem Statement
`NiControllerSequence.accum_root_name` identifies the node whose translation should be extracted as root motion (character movement) rather than applied as animation. Field is parsed but ignored — walk cycles animate in place.

## Affected Files
- `crates/nif/src/anim.rs` — pass accum_root_name through
- `crates/core/src/animation.rs` — delta extraction logic
- `byroredux/src/main.rs` — animation_system handles accum root

## Acceptance Criteria
- [ ] accum_root_name carried through import
- [ ] Horizontal translation extracted as movement delta
- [ ] Walk cycle moves character through world

# SAVE-D3-02: SaveRing cursor is in-memory only — first quicksave after restart clobbers slot 0

Labels: bug import-pipeline medium 

- **Severity**: MEDIUM
- **Dimension**: Disk Format & Durability
- **Data-Loss Class**: irrecoverable-write
- **Location**: `crates/save/src/disk.rs:96-126` (`SaveRing`); install at `byroredux/src/main.rs` (ring size 10, cursor starts at 0)

## Description
`SaveRing` is "stateless on disk beyond the slot files" (its own doc). The cursor lives only in the `SaveState` resource for the session. On every relaunch the ring resets to cursor 0, so the **first** arg-less `save` of a new session writes slot 0 regardless of which slot is the newest on disk — silently overwriting slot 0's prior (possibly most-recent) save. This partially undoes the "ring so a quicksave never clobbers the last good save" goal across sessions.

## Evidence
`SaveRing::new` sets `cursor: 0`; no persistence of the cursor; no "resume from highest existing slot" logic; `list_slots` exists but is not consulted to seed the cursor.

## Impact
Cross-session quicksave can overwrite the previous session's slot-0 save. Within a session the ring works (test `ring_wraps`). MEDIUM: data-loss of slot 0's prior contents on the first post-restart quicksave.

## Suggested Fix
Seed the ring cursor at startup from `list_slots` (e.g. start at `max(existing)+1 mod size`, or persist the cursor in a small sidecar / the newest save's mtime). Document the cross-session behavior either way.

## Completeness Checks
- [ ] **TESTS**: A regression test covers cross-session ring cursor seeding from existing slots

# SAVE-D3-01: rename is not durable — parent directory is never fsynced after rename

Labels: bug import-pipeline medium 

- **Severity**: MEDIUM
- **Dimension**: Disk Format & Durability
- **Data-Loss Class**: irrecoverable-write
- **Location**: `crates/save/src/disk.rs:34-59` (`write_slot`)

## Description
`write_slot` fsyncs the **file** (`sync_all`) and does a byte-exact read-back before `rename` — so half-written *content* can never replace a good slot. But on most filesystems the directory entry created by `rename` is not durable until the **directory** is fsynced. A power cut after `rename` returns but before the dir metadata flushes can lose the rename (revert to the old slot, or lose both). The content-safety guarantee holds; the rename-durability tail does not.

## Evidence
No `File::open(dir)?.sync_all()` after the `fs::rename`. Header doc-comment claims the dance is fully crash-safe.

## Impact
Residual power-cut window where a just-completed save's directory entry is lost. Lower probability than torn content (which is fully handled), hence MEDIUM not HIGH.

## Suggested Fix
After `fs::rename`, open the parent dir and `sync_all()` it (best-effort; ignore `ENOTSUP` on filesystems that don't support dir fsync).

## Completeness Checks
- [ ] **TESTS**: The durability behavior is documented/tested where feasible (dir-fsync best-effort path doesn't error on unsupported FS)

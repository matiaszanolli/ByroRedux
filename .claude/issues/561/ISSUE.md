# #561 — SK-D6-01: CLI single-master only — DLC interiors silently empty

**Severity:** HIGH
**Labels:** bug, high, legacy-compat
**Source:** AUDIT_SKYRIM_2026-04-22.md
**GitHub:** https://github.com/matiaszanolli/ByroRedux/issues/561

## Location
- `byroredux/src/cell_loader.rs:348`, `:438`
- `byroredux/src/scene.rs:60-380` (no `--master` flag)

## One-line
`parse_esm_with_load_order(data, Some(FormIdRemap))` exists (#445) but the binary calls `parse_esm_cells` (no remap). DLC-only loads (Dawnguard, Dragonborn, Update.esm) fail every STAT lookup back to Skyrim.esm.

## Fix sketch
Add repeatable `--master <path>` CLI arg. Thread into `cell_loader::load_cell_with_masters`. Build `LoadOrder` from disk-order, call `parse_esm_with_load_order` with proper `FormIdRemap`.

Promote M46.0 to hard prereq of M32.5.

## Next
`/fix-issue 561`

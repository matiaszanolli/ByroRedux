# RT-3: Skyrim mesh.cache failed contains corrupted control-character mesh paths (string-decode overrun)

- **GitHub**: #1620
- **Severity**: medium
- **Labels**: medium, nif-parser, import-pipeline, legacy-compat, bug
- **Source**: docs/audits/AUDIT_RUNTIME_2026-06-14.md (RT-3)

## Description
Two of the 11 `mesh.cache failed` keys for WhiterunDragonsreach are garbage: `meshes\-e\x03` and `meshes\j.\x01` — an embedded control byte after a one/two-char stem. A Skyrim REFR/NIF mesh-path string is read past its real length (or before a missing terminator); the load key is corrupt and no parse warning fires (silent).

## Location
- Surfaced via `byroredux/src/commands.rs:540-564` (negative-cache dump).
- Origin: Skyrim REFR/NIF mesh-path string decode — candidates `crates/nif/src/` string read or `byroredux/src/cell_loader/references.rs` path assembly.

## Evidence
`/tmp/audit/runtime/skyrim_se-WhiterunDragonsreach.telem.txt` — short stem + control byte, classic read-past-length signature.

## Suggested Fix
Recover affected REFR FormIDs, byte-decode the offending mesh-path subrecord (stride-drift method). Likely wrong-width length field or missing NUL guard. Pair with `/audit-skyrim`.

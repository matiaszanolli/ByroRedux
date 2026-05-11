# SK-D5-NEW-08: BSDynamicTriShape "vanilla never fires" comment is empirically false

**Severity**: LOW (log noise; non-correctness)
**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-05-11.md` Dim 5
**State**: OPEN

## Location

`crates/nif/src/blocks/tri_shape.rs:894-916`

## Evidence

Real-data measurement on `Skyrim - Meshes0.bsa`: **21 140 / 21 140** BSDynamicTriShape blocks fire the warn. Every vanilla SSE FaceGen NIF ships `data_size == 0` on the embedded BSTriShape body, with the head geometry in the trailing Vector4 dynamic array. The path is load-bearing (M41 outfit equip works).

## Fix

Pick one:
1. Downgrade to one-shot `trace!`, update the doc comment.
2. Invert the gate: only fire when `vertices.is_empty() && triangles.is_empty()` (the true silent-fail case).

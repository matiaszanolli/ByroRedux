# FO4-D6-B1: REFR sub-record coverage missing — XTEL/XPRM/XLKR/XPOD/XRMR not parsed (dead doors)

**Issue**: #412 — https://github.com/matiaszanolli/ByroRedux/issues/412
**Labels**: bug, high, legacy-compat

---

## Finding

`crates/plugin/src/esm/cell.rs:468` handles REFR records reading only `NAME`, `DATA`, `XSCL`. FO4 REFR carries several sub-records critical to cell semantics that are silently dropped.

## Missing REFR sub-records

| Sub-record | Content | Without it |
|---|---|---|
| **XTEL** | Teleport destination (door linkage: form ID + pos + rot) | **Every interior door is dead** |
| **XESP** | Enable parent (form ID + flags) | Quest-gated refs render from game-start (partial scope: #349 already tracks this but is Skyrim-focused) |
| **XPRM** | Primitive bounds (trigger boxes / activators without a MODL) | Triggers invisible |
| **XLKR** | Linked refs (NPC ↔ idle marker, door ↔ teleport target) | NPCs don't patrol; doors don't pair |
| **XPOD/XRMR** | Portal/room membership | FO4's cell-subdivided interior culling can't work |
| XRDS | LIGH radius override | Per-ref light tuning lost |
| XOWN/XRNK | Ownership | Gameplay only — deferrable |

## Companion to #349

**#349 (S6-14)** already tracks `XESP` in a Skyrim scope. FO4-D6-B1 broadens to the full FO4-relevant REFR sub-record set; XESP handling should be shared (REFR structure is mostly stable Skyrim → FO4). Proposed: land XESP per #349's Skyrim test, then extend to XTEL/XPRM/XLKR/XPOD/XRMR on the FO4 path here.

## Impact

- **Doors are dead without XTEL**: every interior door's teleport destination comes from XTEL. Without it, activating a door does nothing and the player can never enter.
- Quest-gated refs (guarded doors, corpse placements) render from game-start without XESP resolution.
- FO4 interior portal/room culling can't reduce draw-call count on large vaults without XPOD/XRMR.

## Fix

Extend `cell.rs:468` REFR match arm:

```rust
pub struct PlacedRef {
    pub form_id: FormId,
    pub base_form_id: FormId,
    pub position: [f32; 6],
    pub scale: f32,
    pub enable_parent: Option<(FormId, u32)>,  // XESP
    pub teleport: Option<TeleportDest>,         // XTEL
    pub primitive: Option<PrimitiveBounds>,     // XPRM
    pub linked_refs: Vec<(FormId, FormId)>,     // XLKR (keyword, ref)
    pub room: Option<FormId>,                   // XRMR
    pub portals: Vec<FormId>,                   // XPOD
    pub radius_override: Option<f32>,           // XRDS
}
```

Cell loader needs to resolve teleport destinations and link up linked refs when placing entities. Portal/room culling is a separate renderer work item (M-series).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Coordinate with #349 (XESP for Skyrim). Also applies to FNV REFR — verify FNV already has what it needs or this is a cross-game upgrade.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic REFR with all sub-records round-trips; live test on a vanilla FO4 interior cell verifies door XTEL resolves to another cell's destination ref.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 6 Stage B1.

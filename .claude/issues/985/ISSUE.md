# Issue #985

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/985
**Title**: NIF-D5-ORPHAN-A3: Wire BsConnectPoint::{Parents,Children} consumers — FO4 weapon-mod attachment graph dropped
**Labels**: bug, nif-parser, import-pipeline, medium
**Parent**: #974 (orphan-parse meta) / #869 (original instance)
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: #974 Band A — orphan-parse follow-up
**Severity**: MEDIUM (blocks the FO4 weapon-mod attachment system; FO4 ARMA + ARMO depends on this graph)
**Domain**: NIF import + Equipment ECS

## Description

`BsConnectPointParents` and `BsConnectPointChildren` are dispatched at `crates/nif/src/blocks/mod.rs:543-544` and parsed cleanly into `NifScene`. Neither has any `scene.get_as::<>` consumer in the import pipeline.

These two extra-data blocks together define the **FO4 weapon-mod attachment graph**:

- `BsConnectPointParents` — list of named attach points (e.g. `CON_Magazine`, `CON_Scope`, `CON_Grip`, `CON_Stock`) with relative transform + parent bone name + scale
- `BsConnectPointChildren` — list of named connections back to parent points (the inverse half of the graph)

Together they're the explicit linkage that the FO4 weapon-mod system uses to know where to spawn the muzzle brake mesh on the receiver, the scope mesh on the rail, etc. Without consumer wiring, every modular FO4 weapon imports as a base mesh with NO discoverable attach surface — the weapon-mod system can't function.

## Impact (current behaviour)

Every FO4 ARMA / ARMO that ships modular accessory variants (most pistols, all rifles, power-armor torsos/limbs) is unattachable. M41 equipment system can place the base mesh but can't compose modular variants. The mod-system entry in FO4 .esm OMOD records points at named attach points the engine doesn't know about. Open #973 (FO4-D4-NEW-08-followup MSWP per-shape) is downstream of this — material swap on a per-attach-point basis can't fire until attach points reach the ECS.

## Suggested fix

1. **Component side** — add an `AttachPoints` ECS component (or extend `EquipmentSlots`):

```rust
pub struct AttachPoint {
    pub name: FixedString,       // "CON_Magazine", "CON_Scope", ...
    pub parent_bone: FixedString, // skeleton bone the attach point hangs off
    pub local_transform: Transform,
    pub scale: f32,
}

pub struct AttachPoints(pub Vec<AttachPoint>);
```

2. **Importer side** — when walking a NIF, look for `BsConnectPointParents` extra-data on the root `NiNode` and lift its `connect_points: Vec<ConnectPoint>` into the new component. The `BsConnectPointChildren` data drives the inverse — for modular accessory NIFs whose root references their parent's attach point.

3. **Equipment system side** — when an OMOD references a CON_xxx attach point, look up the named point on the equipped item's `AttachPoints` component, compose the modular accessory transform from `parent.world * point.local_transform * accessory.local`. M41 equipment system bridge.

## Completeness Checks

- [ ] **SIBLING**: both Parents and Children wired in the same PR — they're two halves of the same graph
- [ ] **TESTS**: import a FO4 weapon NIF (e.g. `weapons/10mmpistol/10mmpistol.nif`), verify the named attach points reach the ECS
- [ ] **ECS**: AttachPoints component should be queryable from the equip system without requiring scene-graph traversal
- [ ] **#973 LINK**: cite this issue from #973 so the MSWP per-shape work has the attach-point graph as a prerequisite
- [ ] **DOC**: per-mesh export struct (`ImportedMesh`) should surface the parsed graph so the cell loader picks it up without a second NIF walk

## Source quote (audit report)

> Weapon-mod attachment points (`BsConnectPointParents` — ubiquitous in FO4) parsed but never threaded into the equip / mod system.

`docs/audits/AUDIT_NIF_2026-05-12.md` § HIGH → NIF-D5-NEW-01 (orphan-parse meta).

Related: #974 (meta), #869 (original instance), #973 (MSWP per-shape downstream).


# #1203 — NIF-DIM4-03: BSGeometry skin_instance_ref unconsumed — Starfield NPCs render in bind pose

**Source**: docs/audits/AUDIT_NIF_2026-05-19_DIM4.md (Dim 4, MEDIUM)
**Severity**: medium / Labels: bug, medium, nif-parser, import-pipeline
**State**: OPEN (filed 2026-05-19)

## Cause

`crates/nif/src/import/mesh/bs_geometry.rs:230` literal `skin: None`. No `extract_skin_bs_geometry` sibling exists (NiTriShape + BsTriShape both have one).

## Fix

Implement `extract_skin_bs_geometry` mirroring `extract_skin_bs_tri_shape` at `mesh/skin.rs:174-211`. Starfield uses `BSSkin::Instance` + `BSSkin::BoneData` per nif.xml.

## Game / Risk

Starfield (sole BSGeometry user). LOW risk (currently always returns None).

## Estimated impact

Structural — every Starfield NPC import currently stillborn.

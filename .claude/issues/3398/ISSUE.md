# Starfield CDB Phase 2: extract per-field .mat material data from the Component Database

**Labels**: enhancement,import-pipeline,medium,legacy-compat,game:starfield,nifal

Successor tracker for **Starfield CDB Phase 2 — per-field `.mat` material extraction**, opened per #3395.

## Why this issue exists

#2359 was the previous tracker and is **CLOSED (COMPLETED)**. That closure was correct: its deliverable was the *deferral note plus an invariant test*, not the feature. But `ROADMAP.md` still pointed at it, so the single highest-value remaining Starfield fidelity item read as shipped work to anyone following the chain. This issue takes over that role; the ROADMAP row now points here.

## Current state (verified 2026-08-27 by `/audit-starfield`)

`crates/sfmaterial` parses the Component Database end-to-end — **97 classes / 1,438,780 instances** from `materials\materialsbeta.cdb`, re-measured this audit and byte-identical to the previous baseline. Discovery is correct too: `discover_starfield_cdbs` scans every materials archive and finds **13** CDBs (1 base + 12 DLC/Creation-namespaced under `materials\creations\<plugin>\`), so #1571's scanning fix is load-bearing.

What is missing is the consumer. Nothing walks the parsed tree for per-field data:

- Production never calls `ComponentDatabaseFile::parse` at all — `discover_starfield_cdbs` (`byroredux/src/asset_provider/material.rs:211`) calls only `probe_header`, and `register_starfield_cdb_probe` (`material.rs:631-633`) discards the `CdbHeaderInfo` and increments a counter.
- `merge_external_material`'s `.mat` arm (`material.rs:1073-1125`) is a two-statement stub: it flips `is_pbr = true`, forwards no texture role, no metalness/roughness, no alpha/blend or two-sided/decal state, and correctly self-reports `MergeOutcome::PresenceOnly` (#2709) rather than claiming `Merged`.

Consequence: **every Starfield surface renders on NIF-derived, keyword-classified PBR values** under the Disney BSDF lobe rather than CDB-authored ones.

## The invariant test that pins this

`byroredux/src/asset_provider/tests/starfield_mat.rs:177-188` deliberately asserts the *current* state, and is the thing that must be inverted when Phase 2 lands:

```rust
assert_eq!(outcome, MergeOutcome::PresenceOnly,
    "#2359: the .mat arm resolves the sidecar but must not claim Merged until it actually forwards CDB-authored data");
assert_eq!(mesh.material.textures, MaterialTextureSet::default(),
    "#2359: every MaterialTextureSet role must stay at its default — Phase 1 forwards zero authored texture data from the CDB");
```

## Definition of done

CDB-authored values flow into `ImportedMaterial` **through the existing `merge_external_material` boundary** — never as a render-time fallback, per the NIFAL single-boundary rule (`/audit-nifal`). The observable signal that Phase 2 shipped is the `.mat` arm returning `MergeOutcome::Merged` because a real CDB lookup supplied data, with the two assertions above updated to match.

## Blocked-by / interacts with

- **#3230** — the #3053 CDB gate makes the BGSM/BGEM resolver unreachable for any session with a Starfield CDB registered. Worth settling first or together, since it determines which resolver actually runs on a Starfield session.
- #1289 (Phase 1, shipped), #2709 (`MergeOutcome::PresenceOnly` exists precisely to name the current state), #2353.

## Context

Independently corroborating that the CDB is the *only* vanilla material source: a census of all 129 vanilla + Creation Starfield `.ba2` archives during the 2026-08-27 audit found **0 loose `.bgsm`, 0 loose `.bgem`**, and only 20 `.mat` files (all in third-party Creations). Starfield NIFs name `.bgsm`/`.bgem` references that have no backing files.

See [`docs/audits/AUDIT_STARFIELD_2026-08-27.md`](docs/audits/AUDIT_STARFIELD_2026-08-27.md) — Dimension 3 and the Remaining-Work Chain.

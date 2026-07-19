# Batch: FNV audit doc/traceability fixes (all LOW)

## #1982 FNV-D7-03 — ragdoll plane_min/plane_max silent drop
`byroredux/src/ragdoll.rs` `joint_from_imported` (~158-198) drops plane_min/plane_max
(asymmetric swing) into `..`; symmetric [-cone,cone] applied at crates/physics/src/ragdoll.rs
build_joint. Intentional per docs/engine/physal.md §Known-approximation. Add traceability
comment matching sibling drop sites (#1539/#1718/#1850 use log::warn!). Fix = comment only.

## #1983 DIM4-01 — ROADMAP FNV record total stale
ROADMAP.md:76 says 73,054 structured records; live parse_real_esm reports [FNV] total=77828.
Cite the floor-based parse_real_esm test as source of truth; annotate ROADMAP.md:206 that
14,881 is the NIF-mesh parse count, not an ESM record total. Doc only.

## #1984 D8-01 — CLAUDE.md FNV interior example wrong BSA names
CLAUDE.md Usage example uses bare Meshes.bsa/Textures.bsa which don't exist in vanilla FNV
(real names: "Fallout - Meshes.bsa" / "Fallout - Textures.bsa"). Fix example to quoted names
(match README). Doc only. README + assets/debug_profiles.toml already correct.

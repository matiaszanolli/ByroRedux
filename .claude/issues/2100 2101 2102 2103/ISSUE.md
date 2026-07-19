# Batch: Starfield CDB material path (SF-D3 audit) + doc

## #2100 MEDIUM SF-D3-AUDIT-01 — full CDB tree parsed+retained just for a bool
material.rs:236-267 load_starfield_cdb/has_starfield_cdb; reader.rs:29-90 parse.
Only consumer is has_starfield_cdb()=!sf_cdbs.is_empty(). Full 1.44M-elem tree parsed
(9.7s debug) + retained in Arc forever, unread (Phase 2 unbuilt). Fix: header-only presence
probe (peek_magic/parse_header) OR drop instances after check. Crate: byroredux + sfmaterial.

## #2101 LOW SF-D3-AUDIT-02 — read_primitive_string no trailing-NUL trim
reader.rs:535-539. Gibbed uses trimNull=true; Rust reads len bytes raw. Truncate at first
0x00 before lossy UTF-8. Add test. Latent (Phase 1 doesn't read inline strings). Crate: sfmaterial.

## #2102 LOW SF-D3-AUDIT-03 — peek_magic test-only, not in discovery
reader.rs:95-101 peek_magic; material.rs:23-45 discovery (path-based → full parse). Wire
peek_magic for cheap reject OR document as public probe. Crate: sfmaterial + byroredux.

## #2103 LOW doc DIM4-STARFIELD-01 — baseline count stale by 16
docs/engine/starfield-esm-phase0-baseline.md:174 count 1,971,151 vs live 1,971,135 (commit
2dc43106 skips 16 Deleted-flag tombstone REFRs). Add "superseded" note. Doc-only.

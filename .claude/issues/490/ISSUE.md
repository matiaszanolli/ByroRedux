# Issue #490

FO4-BGSM-1: new crates/bgsm crate — BGSM v2 + BGEM v2 parser + template inheritance resolver

---

## Parent
Split from #411. **Foundational — blocks #BGSM-2 through #BGSM-5.**

## Scope

New workspace member `crates/bgsm/`. Standalone binary-format parser for Fallout 4's external PBR-ish material files.

### Format (from #411 + nifly/BGSM.h verification)

- **BGSM v2** (FO4): common prefix at 0x00-0x3E (tile_flags, uv_offset/scale, alpha, blend/test, z_write/test, 7× u8 flags, refraction, env_mapping, env_scale) + 9 texture slots + lit-specific trailer including `rootmaterial_path` length-prefixed string.
- **BGEM v2**: shares prefix + texture slots, then diverges into `base_color[3]`, falloff, soft-depth.

Magic: `"BGSM"` / `"BGEM"` + u32 version (2 for FO4).

### Deliverables

- `crates/bgsm/Cargo.toml` — standalone crate, `anyhow` + byteorder deps (workspace-consistent)
- `crates/bgsm/src/lib.rs` — public API: `Bgsm`, `Bgem`, `MaterialFile` enum, `parse_bgsm`, `parse_bgem`, `parse` (dispatches on magic)
- `crates/bgsm/src/template.rs` — **template inheritance resolver**: follows `rootmaterial_path` chain, merges parent fields bottom-up (child overrides parent). **LRU cache mandatory** — creature BGSM chains share templates and re-parsing dominates load time otherwise.
- Workspace `Cargo.toml` update — register new member

### Deferred

- FO76 v20 / Starfield v21/22 — follow-up issue. FO4 v2 is the P0 target.

## Completeness Checks

- [ ] **TESTS**: unit tests with synthetic 0-byte / truncated / bad-magic / v2 happy-path inputs
- [ ] **TESTS**: template resolver test — 3-level chain resolves correctly, cache hit rate verified
- [ ] **SIBLING**: no touch to NIF crate yet — `crates/bgsm` has zero reverse deps after this issue lands
- [ ] **DOCS**: crate-level doc comment with format citation

## Reference

nifly `BGSM.h` (niftools) for field layout authority. Samples at `/tmp/audit/fo4/sample_{0,1,2}.bgsm` + `sample_{0,1}.bgem` per audit AUDIT_FO4_2026-04-17.md.

## Out of scope

- Engine integration (tracked in #BGSM-3 + #BGSM-4)
- GPU plumbing (#BGSM-3)
- Fragment shader (#BGSM-5)
- Corpus-scale test (#BGSM-2)

# #3741 — TD2-2026-08-30-01: the game-data path helper built to end path hardcoding is `pub(crate)`, so its own package's integration tests re-hardcode 42 Steam roots (134 workspace-wide)

**Labels**: bug, medium, tech-debt, esm-plugin

---

- **Severity**: MEDIUM
- **Dimension**: 2 — Duplication
- **Location**: `crates/plugin/src/esm/test_paths.rs`; visibility declared at `crates/plugin/src/esm/mod.rs` (`pub(crate) mod test_paths;`); the 42 open-coded sites at `crates/plugin/tests/parse_real_esm.rs`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD2-2026-08-30-01`), HEAD `64f64480`

## Description

`test_paths.rs` was created by **#1058** for exactly this. Its own module doc states the
intent:

> "Pre-#1058 each test hardcoded the audit author's Steam install path; this module
> centralises the override shape so every test resolves the same way."

It provides 12 `pub(crate) fn` accessors, each an env-var override
(`BYROREDUX_<GAME>_DATA`) falling back to the reference machine's Steam path.

**It is declared `pub(crate) mod test_paths;`.** An integration test under `tests/` is a
*separate crate*, so `crates/plugin/tests/parse_real_esm.rs` — in the same package —
structurally cannot call it. The result: that one file re-hardcodes the same Steam roots
**42 times**.

Workspace-wide the literal `"/mnt/data/SteamLibrary/steamapps/common/..."` appears
**134 times** at HEAD (the report measured 119 three days earlier — it is still growing)
across `crates/plugin`, `crates/nif`, `crates/bsa`, `crates/spt`, `crates/audio`,
`crates/facegen`, `crates/sfmaterial` and `byroredux/tests`, covering 7 distinct game
roots.

## Amplification — why this is not the default LOW

This is duplicated logic with *divergent* behaviour, not just repeated text.
`test_paths.rs` guarantees every accessor consults its env override first; the open-coded
sites each re-implement that override by hand — some do
(`std::env::var("BYROREDUX_FNV_DATA").unwrap_or(...)`), and whether *all* do is
unverifiable by inspection at that scale. A site that forgets the env var is a test that
silently skips on any machine but one, which is the failure mode #1058 set out to remove
and did not finish removing.

## Suggested Fix — the module already names its own consolidation site

`test_paths.rs`'s own doc says *"promoting to a workspace-level utility crate is out of
scope for the issue that introduced this module (#1058)"*. That is the fix, one increment
later. Two options, in order of preference:

1. A tiny `crates/test-paths` dev-dependency crate carrying the 12 accessors plus the
   `nif/tests/common::Game` `default_path()` / `mesh_archive()` convention it already
   mirrors. Every crate lists it under `[dev-dependencies]`; all 134 literals collapse to
   7 constants in one file. *(medium)*
2. Cheaper interim: change `pub(crate) mod test_paths` to `pub mod test_paths` gated
   behind a `test-paths` feature enabled in the plugin crate's own `[dev-dependencies]`
   self-reference — unblocks `parse_real_esm.rs`'s 42 sites immediately without touching
   the other crates. *(small)*

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all 7 crates carrying hardcoded roots)
- [ ] **TESTS**: A regression test pins this specific fix

# FNV-2026-08-26-D8-02

**Issue**: #3346
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 8 — Real-Data Validation & Bench
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/boot.rs:1686-1717`, `assets/debug_profiles.toml:42-59`,
`.claude/commands/audit-fnv/SKILL.md:141`, `ROADMAP.md:160`

**Premise verified**: the CWD claim itself still holds — `build_texture_provider`
(`byroredux/src/asset_provider/texture.rs:219-247`) passes the raw arg string to
`Archive::open`, which hands it to `std::fs::File::open`, so bare names resolve
against process CWD. Confirmed live.

But `expand_game_profile_args` (`boot.rs:1694-1716`) synthesizes **absolute** paths:

```rust
let join_arg = |archive: &str| -> String { data_dir.join(archive).to_string_lossy().into_owned() };
args.push("--esm".to_string());
args.push(join_arg(&entry.esm));
for bsa in &entry.default_bsas { args.push("--bsa".to_string()); args.push(join_arg(bsa)); }
```

with `[profiles.fnv]` already carrying `esm = "FalloutNV.esm"`,
`default_bsas = ["Fallout - Meshes.bsa"]`,
`default_textures_bsas = ["Fallout - Textures.bsa"]`. So `--game fnv` is entirely
CWD-independent and cannot mistype an archive name, and the sibling auto-load supplies
`Textures2` (verified below). Both the ROADMAP repro note and the skill still teach
only the fragile bare-name + `cd` form.

**Impact**: every auditor and every doc reader is steered to the one invocation shape
that has a documented silent-failure mode, when a shipped alternative has neither.
Low severity because the failure is now loud (see Regression guards, #1776).

**Fix sketch**: add the `--game fnv` form alongside the bare-name one in
`ROADMAP.md:160` and `SKILL.md:141`, noting the bench harness deliberately keeps the
bare-name + `cd` shape for apples-to-apples continuity with the historical record.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

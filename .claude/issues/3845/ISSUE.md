# #3845: TD2-2026-09-05-02: `parse_skyrim_shader_base` is the shared Skyrim+ shader head, but its two inline twins got #2603's gap-band predicates and it did not

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-02) via `/audit-publish`, 2026-09-05. Labels: `medium,nif-parser,nif,game:skyrim,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3845 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD2-2026-09-05-02), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: MEDIUM (promotion trigger: divergent bug-fix history)
- **Dimension**: 2 — Logic Duplication
- **Location**:
  - `crates/nif/src/blocks/shader.rs` — `parse_skyrim_shader_base` (the helper; consumed by `BSSkyShaderProperty::parse` and `BSWaterShaderProperty::parse`)
  - `crates/nif/src/blocks/shader.rs` — `BSLightingShaderProperty::parse_fo4` (inline copy)
  - `crates/nif/src/blocks/shader.rs` — `BSEffectShaderProperty::parse` (inline copy)
- **Status**: NEW
- **Description**: All three read the identical six-field Skyrim+ shader head —
  `shader_flags_1`/`shader_flags_2` (typed `u32` pair), then the
  `sf1_crcs`/`sf2_crcs` CRC arrays, then `uv_offset` and `uv_scale`. A helper
  for exactly this sequence already exists in the same file
  (`parse_skyrim_shader_base` → `type SkyrimShaderBase`), but the two largest
  consumers hand-roll it. Because they are separate copies, `70f1bb74`'s #2603
  work — replacing the raw BSVER literal comparisons with the named
  `bsver::carries_typed_shader_flags` / `carries_crc_shader_flags` predicates
  that encode the BSVER-131 "neither encoding present" gap band — landed on the
  two inline copies and left the helper on the old literal gate.
- **Evidence**:
  - Helper, unchanged since before #2603:
    `if bsver < crate::version::bsver::FO4_CRC_FLAGS { (read_u32, read_u32) } else { (0, 0) }`
    — `FO4_CRC_FLAGS == 132`, so this reads the 8-byte pair at `bsver == 131`.
  - Both inline copies:
    `if crate::version::bsver::carries_typed_shader_flags(bsver) { … }` —
    `carries_typed_shader_flags(b) == (b <= FALLOUT4)`, i.e. `b <= 130`, so
    they read *nothing* at 131.
  - `crates/nif/src/version.rs` pins the partition:
    `assert!(!carries_typed_shader_flags(FO4_SHADER_GAP));`
    `assert!(!carries_crc_shader_flags(FO4_SHADER_GAP));` — 131 carries neither.
    `is_shader_flag_gap` exists solely to name that band.
  - `git show 70f1bb74 -- crates/nif/src/blocks/shader.rs` shows the two inline
    gates being rewritten from `bsver <= FALLOUT4` / `bsver >= FO4_CRC_FLAGS`
    to the predicates; `parse_skyrim_shader_base` is absent from the diff.
- **Impact**: Latent, not live: at `bsver == 131` a `BSSkyShaderProperty` or
  `BSWaterShaderProperty` would over-consume 8 bytes and drift the stream for
  the rest of the block. BSVER 131 is a dev-stream band that ships no game
  content (the version.rs doc comment says so), so nothing in the corpus hits
  it today. The real cost is that the codebase now holds three *different*
  answers to "does this BSVER carry typed shader flags", and the test that pins
  the partition (`bsver_shader_flag_band_tests`) validates the predicates, not
  the one parse site that bypasses them.
- **Related**: #2603 (CLOSED, `70f1bb74`), #409 (the original gap-band
  discovery), #713 (which created `parse_skyrim_shader_base`).
- **Suggested Fix**: In `crates/nif/src/blocks/shader.rs`, change
  `parse_skyrim_shader_base`'s two gates to
  `bsver::carries_typed_shader_flags(bsver)` /
  `bsver::carries_crc_shader_flags(bsver)`, then route
  `BSLightingShaderProperty::parse_fo4` and `BSEffectShaderProperty::parse`
  through the helper for the six shared fields (both continue their own tails
  unchanged after it). That leaves exactly one copy of the gate, already covered
  by `bsver_shader_flag_band_tests`.
- **Effort**: small (≤2 h)

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved

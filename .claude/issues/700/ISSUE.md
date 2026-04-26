# Issue #700: O2-2: misleading comment in BSA v103 reader claims 'different flag semantics for bits 7-10'

**Severity**: LOW
**File**: `crates/bsa/src/archive.rs:186-187`
**Dimension**: BSA v103 Archive

```rust
// Bit 0x100 means "embed file names" only in v104+ (FO3/Skyrim).
// Oblivion v103 uses different flag semantics for bits 7-10.
let embed_file_names = version >= 104 && archive_flags & 0x100 != 0;
```

The comment "Oblivion v103 uses different flag semantics for bits 7-10" is **speculative and wrong**. The UESP / libbsarch documented semantics for archive-flag bit 0x100 in v103 is **"Xbox archive"** (a single bit nobody parses on PC), not a different layout for bits 7-10. Behavior is correct (v103 ignores 0x100 for embed-name purposes — confirmed by 100% extraction across the 17 vanilla archives, several of which set bit 0x100 for the Xbox compile path). Comment is what's wrong.

**Same status as 04-17 M-2.** Line numbers shifted (162-164 → 186-187) post-#586 but the comment text is bit-for-bit identical.

**Fix**: Rewrite as
```rust
// Bit 0x100 has different meaning across versions:
//   v103 (Oblivion): "Xbox archive" — irrelevant on PC.
//   v104+ (FO3/Skyrim): "embed file names" — extract path skips a
//     bstring prefix in each file body.
// Source: UESP `Oblivion_Mod:BSA_File_Format#Archive_Flags`,
// libbsarch `bsa_open.cpp` flag table.
let embed_file_names = version >= 104 && archive_flags & 0x100 != 0;
```

3 lines, comment-only. No behavior change.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*

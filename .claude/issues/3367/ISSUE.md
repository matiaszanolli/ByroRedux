# SKY-2026-08-27-D5-01: the per-file "bit 31 = embed-name toggle" semantics is unsourced and diverges from the reference implementation

Labels: low,import-pipeline,documentation,doc-rot,game:skyrim,legacy-compat

- **Severity**: LOW
- **Confidence**: PLAUSIBLE (code-read + reference-impl comparison; the *data* half —
  that the bit is never set on shipped content — is CONFIRMED)
- **Location**: `crates/bsa/src/archive/open.rs:250`, `crates/bsa/src/archive/extract.rs:60`,
  doc comment `crates/bsa/src/archive/mod.rs:64-73`
- **Description**: The reader treats bit 31 (`0x80000000`) of the file record's size
  word as a *per-file embed-name override* that XORs against the archive-level
  `0x100` flag, and the doc comment states that meaning as established fact
  ("Bit 31 (0x80000000) of the on-disk size word … Mixed-mode BSAs … need this
  toggle XOR'd against the archive-level `embed_file_names`"). No spec or reference
  implementation available on this machine assigns that meaning to bit 31. openmw —
  the only full third-party BSA reader in `/mnt/data/src/reference/` — declares exactly
  one size flag and deliberately leaves bit 31 inside the size value:

  ```cpp
  // reference/openmw/components/bsa/compressedbsafile.hpp:73-76
  enum FileSizeFlags { FileSizeFlag_Compression = 0x40000000, };
  // reference/openmw/components/bsa/compressedbsafile.cpp:267
  size_t size = fileRecord.mSize & (~FileSizeFlag_Compression);
  ```

  and it drives the name skip purely off the archive flag
  (`if ((mHeader.mFlags & ArchiveFlag_EmbeddedNames) != 0)`, `compressedbsafile.cpp:271`).
  The referenced internal issue (#616 / SK-D2-03) is an audit finding, not an
  external source, so the claim currently rests on nothing citable — a direct hit on
  the project's NO-GUESSING doctrine.
- **Evidence**: I re-parsed every file record in every BSA of four installed games
  and counted the bit independently of `BsaArchive`:

  ```
  Skyrim SE   (23 archives, 172,918 files):  bit31 set on 0 files
  Fallout NV  (21 archives):                 bit31 set on 0 files
  Fallout 3   (16 archives):                 bit31 set on 0 files
  Oblivion    (17 archives):                 bit31 set on 0 files
  ```

  Bit 30 by contrast *is* exercised on real data (Oblivion
  `DLCShiveringIsles - Meshes.bsa`: 3,014 files; `- Textures.bsa`: 1,869 files;
  `Knights.bsa`: 217), so the compression XOR has genuine on-disk coverage while the
  bit-31 path has none outside the three synthetic tests at
  `archive/tests.rs:385/419`.
- **Impact**: Zero on any vanilla or Creation Club content across all four games.
  Only reachable on a third-party/modded v105 archive whose packer sets bit 31. If
  that bit means something other than "flip embed-name", the extractor consumes a
  1+N-byte bstring prefix that isn't there and returns a body shifted by that many
  bytes — a silent, non-erroring corruption (a NIF would fail its magic check
  loudly, but a DDS or a raw asset would not).
- **Suggested Fix**: Either (a) cite a real source in the doc comment (UESP
  `Skyrim Mod:Archive File Format`, BSArch, or libbsarch), or (b) match openmw and
  ignore bit 31, downgrading it to a `log::debug!` "unknown size flag set on '<path>'"
  so a real-world instance surfaces instead of being silently acted on. Keep the
  `& 0x3FFFFFFF` mask either way — it is harmless (largest single decompressed file
  across all Skyrim BSAs is 67,308,868 bytes, `shadersfx\shaders011.fxp`).
- **Related**: checked the 300-issue dedup baseline (84 open, fetched 2026-08-27) — no open or closed issue mentions
  bit 31 / `0x80000000` / the embed-name toggle. #3348 (red `--ignored` doctests) is
  unrelated and out of scope per CONTEXT.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*

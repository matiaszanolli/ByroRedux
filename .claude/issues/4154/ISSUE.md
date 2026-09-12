# NIF-D3-2026-09-11-01: d5_coverage coverage-probe tool silently skips all .bto/.btr files

URL: https://github.com/matiaszanolli/ByroRedux/issues/4154
Labels: bug, nif-parser, medium, nif, game:fo4, game:fo76, test-gap

---

**Severity**: MEDIUM
**Dimension**: 3 — Block Dispatch Coverage
**Game Affected**: Fallout 4, Fallout 76 (any archive whose content is `.bto`/`.btr`)
**Location**: `crates/nif/examples/d5_coverage.rs:91,116`; contrast with `crates/nif/src/corpus.rs:20-28` (`is_nif_entry`)
**Status**: NEW

**Description**: The tool this project's own doc comments call "the LIVE source of truth for dispatch coverage" hardcodes a `.nif`-only filter instead of the shared `corpus::is_nif_entry`. Live-tested against FO76's `GeneratedMeshes02.ba2` (2,049 files, 100% `.bto`, 0 `.nif`): the tool reports "0 NIFs" and a vacuous "100.0000% coverage" for an archive it never opened a single file from. `crates/nif/tests/block_coverage_baselines.rs:126` has the identical duplicated filter (currently harmless — its one caller is Oblivion-only).

**Evidence** (`examples/d5_coverage.rs:88-91` / `:113-116`):
```rust
let nifs: Vec<String> = archive
    .list_files()
    .iter()
    .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
    ...
```
vs. `corpus.rs:20-28`:
```rust
pub const NIF_ENTRY_EXTENSIONS: &[&str] = &[".nif", ".bto", ".btr"];
pub fn is_nif_entry(path: &str) -> bool { ... }
```

**Impact**: No production impact (`parse_real_nifs.rs`/`per_block_baselines.rs` correctly use `corpus::is_nif_entry`) — but this specific audit/diagnostic tool produces a clean-looking, vacuously-true coverage report for `.bto`/`.btr`-dominant archives, which is exactly the failure mode that could mislead a future audit pass.

**Suggested Fix**: Replace both hardcoded filters with `byroredux_nif::corpus::is_nif_entry`.

## Completeness Checks
- [ ] **SIBLING**: `crates/nif/tests/block_coverage_baselines.rs:126`'s identical duplicated filter fixed in the same pass
- [ ] **TESTS**: Re-run the coverage probe against a `.bto`/`.btr`-dominant archive to confirm it now reports real coverage instead of a vacuous 100%


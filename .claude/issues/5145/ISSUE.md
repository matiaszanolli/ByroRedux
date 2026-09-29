# #5145: TOOL-D5-2026-09-29-01: `esm_opens` reads the entire ESM to check a 24-byte header, so the launcher's startup and Rescan read about 3.2 GB on the UI thread

**Labels**: medium, bug, tech-debt, performance

**Source report**: `docs/audits/AUDIT_TOOLING_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: Install Detection (exposure: every launcher user at startup and on Rescan; `byro-detect`)

## Location
- `crates/game-detect/src/validate.rs` `esm_opens` (`std::fs::read(path)` → `EsmReader::read_file_header`)
- Callers: `tools/byro-launcher/src/state.rs` (`LauncherState::load`, `refresh` from the egui `update` closure in `app.rs`), `tools/byro-detect/src/main.rs`

## Description
Introduced by the #4759 fix (`63c0aee3b`). `validate()` runs once per detected game, and each run slurps the whole main plugin to parse its TES4 record. Installed sizes on the dev box: Starfield.esm 1,457,098,709 B; SeventySix.esm 925,469,444 B; Fallout4.esm 330,776,576 B; Skyrim.esm 249,752,131 B; FalloutNV.esm 245,650,747 B. The launcher therefore reads about 3.2 GB synchronously before its first frame, and again on every Rescan click, with peak RSS around 1.46 GB. The full read buys nothing: the header parse cannot detect mid-file truncation, only a bad or short first record.

## Evidence
`let bytes = std::fs::read(path).map_err(|error| error.to_string())?;` followed by `EsmReader::new(&bytes).read_file_header()`.

## Impact
A cold-cache launcher start or a Rescan stalls the UI for seconds on an SSD and far longer on an HDD. A 1.5 GB allocation spike is a poor fit for a tool whose stated job is to run when the engine cannot.

## Related
#4759. Label gap: game-detect / launcher have no own label → `tech-debt`.

## Suggested Fix
Read the 24-byte record header, bound the TES4 payload (e.g. ≤ 1 MiB), and parse from that prefix only.

Validated at HEAD 9fcfdc3fc: `esm_opens` in `validate.rs` still calls `std::fs::read(path)` on the whole file.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (e.g. a truncated-after-header fixture still validates; a huge file is not fully read)

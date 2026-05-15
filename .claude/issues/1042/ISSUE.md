# Issue #1042: Tech-Debt: NIF version & BSVER bare literals — name them and sweep [batch]

**State:** OPEN
**Labels:** enhancement, nif-parser, medium, tech-debt

## Description

NIF version codes (`NifVersion(0x14010001)` etc.) and BSVER thresholds (`bsver > 130`) are bare hex/decimal literals scattered across the block parsers. `crates/nif/src/version.rs` already defines some named constants, but most call sites still use hex longhand.

## Findings

| ID | Pattern | Count | Where |
|----|---------|------:|-------|
| TD4-001 | Bare `NifVersion(0x...)` literals | 143 sites | `crates/nif/src/blocks/` (13 files) |
| TD4-008 | BSA/BA2 version allowlists as bare literals | 8 sites | `crates/bsa/src/{archive,ba2}.rs` |
| TD4-009 | Bare `bsver` literal compares | 106 sites | `crates/nif/src/blocks/` |
| TD4-016 | `NifVersion(0x14010001)` (string table boundary) | 2 sites | (now `STRING_TABLE_THRESHOLD`) |
| TD4-019 | `user_version_2 > 130 / < 131` (FO4 BSVER boundary) | 1 site | `crates/nif/src/header.rs:127-130` |

## Status at Time of Issue

Already done:
- `STRING_TABLE_THRESHOLD` constant in version.rs ✓
- `bsver` module with FO3_FNV, SKYRIM_LE, SKYRIM_SE, FALLOUT4, FO76, STARFIELD ✓

Remaining:
- Additional NifVersion constants for all ≥2-site hex codes
- Intermediate bsver constants (14, 24, 26, 28, 131, 132, 140, 152)
- BSA/BA2 version named constants in `crates/bsa/`
- Mechanical replace of all bare literals with named constants

## Completeness Checks

- [ ] `grep -RnE 'NifVersion\(0x[0-9a-fA-F]+\)' crates/nif/src/blocks/` returns ≤ 5
- [ ] All 23 of `V10_1_0_0`'s previous hex-longhand sites now use the constant
- [ ] `crates/bsa/src/{archive,ba2}.rs` use named version constants throughout
- [ ] Tests still pass

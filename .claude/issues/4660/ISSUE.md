# PAR-D4-2026-09-21-02: Strict-lane (#3850) holes: several harnesses still turn a missing archive, a broken reader or an unset variable into a green skip

Labels: medium,bug,import-pipeline,test-gap

## Description
Several real-data harnesses still resolve "archive present but unopenable", or "env var absent/misnamed", to a green skip even under `BYROREDUX_REQUIRE_GAME_DATA=1` (#3850):

- **bgsm** (`crates/bgsm/tests/parse_all.rs:171-185`). `open_materials_archive` returns `None` (test passes) when `Fallout4 - Materials.ba2` is absent **or `Ba2Archive::open` fails**, even under REQUIRE.
- **facegen** (`crates/facegen/tests/parse_real_facegen.rs:149-176`). `Game::extract` uses `BsaArchive::open(..).ok()?` / `.extract(..).ok()`, surfacing as "data not available; skipping". `for_each_game` only `eprintln!`s when zero games ran at all.
- **bsa_real / ba2_real / csg_real** (`crates/bsa/tests/bsa_real.rs:104-108`). A missing named archive after `require_game_data` passes is "Skipping" plus `return`.
- **bsa in-crate tests** (`crates/bsa/src/archive/tests.rs:172-244`). The 11 `#[ignore]`d FNV/SSE tests never call `require_game_data` at all.
- **hkx** (`crates/hkx/src/animation.rs:1171-1181`). Its real-data tests skip and return under REQUIRE, and read `BYROREDUX_SKYRIM_DATA` while `real-data-gates.yml:62` and `bsa_real` use `BYROREDUX_SKYRIMSE_DATA` — the repo-wide env-var split noted in AUDIT_UI_2026-08-27, still unfiled. A set-but-wrong-spelled override is not binding either.
- **menuxml** (`crates/menuxml/tests/vanilla_corpus.rs:48-64`). Plain `#[test]` with no default path or `require_game_data` call.

Verified unchanged at HEAD `ee6d3fb39`: `open_materials_archive` still returns `None` on an open failure (`Ok(a) => Some(a), Err(e) => { eprintln!(...); None }`); `crates/hkx/src/animation.rs` still reads `BYROREDUX_SKYRIM_DATA` while `bsa_real.rs`/`real-data-gates.yml` use `BYROREDUX_SKYRIMSE_DATA`; `vanilla_corpus.rs` is still a bare `#[test]`.

## Evidence
```rust
match Ba2Archive::open(&archive_path) {
    Ok(a) => Some(a),
    Err(e) => { eprintln!("skipping: failed to open {:?}: {}", archive_path, e); None }
}
```

## Impact
The dangerous cases are bgsm and facegen: a regression in the archive readers those tests depend on reads as a skip, so the strict lane goes green on exactly the failure it exists to catch.

## Related
#3850 (the strict-lane guard this generalises), #3014, #3741

## Suggested Fix
- Under REQUIRE, turn every "archive absent" or "open failed" branch into a panic that names the path.
- Route the hkx and menuxml resolvers through the same `data_dir()` / `require_game_data` shape as `bsa_real`.
- Pick one Skyrim SE env-var spelling repo-wide.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D4-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
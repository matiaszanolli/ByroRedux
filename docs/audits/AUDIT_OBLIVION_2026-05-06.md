# Oblivion Compatibility Audit — Dimension 2 (BSA v103 Archive)

**Date**: 2026-05-06
**Scope**: Dimension 2 only (BSA v103 Archive). Regression guard, not fresh investigation.
**Method**: Static review of `crates/bsa/src/archive.rs` + tests, `cargo test -p byroredux-bsa`, single end-to-end extraction smoke test against a real v103 archive on disk.

## Executive Summary

**Regression guard verdict: PASS.**

BSA v103 (Oblivion) archive open + extract is materially clean. All six checklist invariants pass at current line numbers. The 45 unit tests + 1 BA2 integration test in the `byroredux-bsa` crate are green; the v104/v105 on-disk regression tests stay `#[ignore]`'d (as intended). A live smoke test extracted `meshes\clutter\upperclass\uppersilvergoblet01.nif` from `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion - Meshes.bsa` (20,182 files indexed) and produced a 35,739-byte buffer whose first 20 bytes are exactly `Gamebryo File Format`. The zlib path on the v103 dispatch arm is healthy.

No CRITICAL / HIGH / MEDIUM / LOW findings are NEW for this dimension. The single carry-over MEDIUM (`#690`, "zero on-disk v103 regression test coverage") is unchanged since `AUDIT_OBLIVION_2026-04-25` — surfaced again for visibility, not re-filed.

The pre-2026-04-17 framing of "v103 is broken" remains a stale-premise hazard. This report is explicit about that — see the **Stale-framing risk** section below.

## Verified Invariants

| # | Invariant                                                                                       | File:line                                                | Verdict |
|---|-------------------------------------------------------------------------------------------------|----------------------------------------------------------|---------|
| 1 | v103 header recognition — version byte 103 is in the accepted set                               | `crates/bsa/src/archive.rs:164-173`                      | PASS    |
| 2 | 16-byte folder-record stride for v103 (vs 24 B for v105); v104 also 16 B                        | `crates/bsa/src/archive.rs:225` (`if version == 105 { 24 } else { 16 }`) | PASS    |
| 3 | v103 file-record offset read as u32 at `[12..16]` (not u64 at `[16..24]`)                       | `crates/bsa/src/archive.rs:256-261`                      | PASS    |
| 4 | Bit 0x100 archive flag treated as Xbox-archive (no-op on PC) for v103, embed-name only for v104+ | `crates/bsa/src/archive.rs:192-200`                      | PASS    |
| 5 | v103 takes the zlib decompression path (LZ4 frame is gated on `version >= 105`)                 | `crates/bsa/src/archive.rs:603-613`                      | PASS    |
| 6 | Folder + file hash functions match real archives (verified via FNV `glover.nif` stored hash)    | `crates/bsa/src/archive.rs:30-107`, test `genhash_file_matches_stored_fnv_meshes_bsa_entry` at `:752-764` | PASS    |
| 7 | `cargo test -p byroredux-bsa` green (45 unit + 1 ba2_real + 0 bsa_real, 21 ignored Steam-disk)  | command output                                           | PASS    |
| 8 | End-to-end smoke: open + extract one NIF from `Oblivion - Meshes.bsa` returns Gamebryo magic    | `oblivion_extract` example, run 2026-05-06               | PASS    |

### Notes on each invariant

**(1) Version dispatch.** `let version = u32::from_le_bytes(header[4..8].try_into().unwrap()); if version != 103 && version != 104 && version != 105 { return Err(...); }`. Allowlist is explicit; no fall-through. The dispatch contract is then exercised everywhere via `if version == 105 { ... } else { ... }` checks (folder record stride, offset width) and `if self.version >= 105 { ... } else { ... }` (codec selection), so the v103 arm is always the same path FNV/Skyrim LE take for those branches.

**(2) Folder-record stride.** The stride is the only on-disk quantity that distinguishes v104 from v105 (both v103 and v104 are 16 bytes; v105 widens to 24 because the offset becomes u64 with a u32 padding word in front). The `else` arm of the ternary at line 225 is the v103 path, and matches both the inline doc comment at line 113 (`v103: Oblivion (16-byte folder records, zlib compression)`) and the offset-read at line 260.

**(3) Folder offset read.** Lines 256-261 select between u64-at-`[16..24]` (v105) and u32-at-`[12..16]` (v103/v104). The expected-offset bias check at line 311 (`folder.offset.saturating_sub(_total_file_name_length as u64)`) runs in debug builds for both paths. Pre-`#586` this code already existed; no drift since.

**(4) Bit 0x100 semantics.** Line 200: `let embed_file_names = version >= 104 && archive_flags & 0x100 != 0;`. The `version >= 104` clamp is the load-bearing fix that keeps Oblivion's vanilla v103 archives (which do set 0x100, per the comment at line 193-199, because that bit is `Xbox archive` on Oblivion) from being misinterpreted as embed-name archives. Pre-fix, every Oblivion BSA would have tried to skip a bstring path prefix that isn't there and corrupted the first byte of every payload.

**(5) Codec selection.** Line 603: `if self.version >= 105 { LZ4 frame } else { zlib }`. v103 falls through to the zlib branch alongside v104. The 4-byte uncompressed-size header read at line 567-575 + post-decompression length sanity check at line 626-636 are version-agnostic and correct for both.

**(6) Hash functions.** `genhash_folder` and `genhash_file` are shared across v103/v104/v105 (the algorithm is identical — see UESP / libbsarch reference). The pinned regression test `genhash_file_matches_stored_fnv_meshes_bsa_entry` (line 752) uses a real FNV stored hash but the algorithm is the one Oblivion's authoring tools wrote into v103 archives. The pre-`#449` extension-rolling-hash bug would have manifested as 119k validation warnings per Oblivion archive open in debug builds; current source produces zero.

**(7) Test status.** Output of `cargo test -p byroredux-bsa`:

```
test result: ok. 45 passed; 0 failed; 12 ignored; 0 measured; 0 filtered out
test result: ok. 1 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out  (ba2_real.rs)
test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out  (bsa_real.rs)
```

The 12 ignored unit tests are Steam-disk gated (`#[ignore]` + `skip_if_missing` / `skip_if_skyrim_missing`). The 9 ignored integration tests in `bsa_real.rs` and `ba2_real.rs` are `BYROREDUX_*_DATA`-gated. None of the ignored set targets v103 — see Carry-Over Findings below.

**(8) Live smoke test.** Run on 2026-05-06:

```
$ cargo run -q -p byroredux-bsa --example oblivion_extract -- \
    "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion - Meshes.bsa" \
    "meshes\\clutter\\upperclass\\uppersilvergoblet01.nif" \
    /tmp/audit/oblivion_v103_smoke.nif

opened /mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion - Meshes.bsa (20182 files)
extracted 35739 bytes, first 20: [47, 61, 6d, 65, 62, 72, 79, 6f, 20, 46, 69, 6c, 65, 20, 46, 6f, 72, 6d, 61, 74]
wrote /tmp/audit/oblivion_v103_smoke.nif
```

Bytes `47 61 6d 65 62 72 79 6f 20 46 69 6c 65 20 46 6f 72 6d 61 74` = `Gamebryo File Format`. Open + zlib decompress + payload return are all healthy on the v103 path.

## Misleading Diagnostics Watch

Spec-required scan: anything in the live codebase that says "v103 is broken" when the runtime path actually works.

- `crates/bsa/src/archive.rs` — clean. The single comment at line 193-199 correctly identifies bit 0x100 as Xbox-archive on Oblivion. The pre-`AUDIT_OBLIVION_2026-04-25` "v103 uses different flag semantics for bits 7-10" misleading comment (logged then as O2-2 LOW) is no longer present in the source — confirmed by grep for `bits 7` / `bits 7-10`.
- `crates/bsa/src/lib.rs` — clean. Module docstring lists v103 (Oblivion) as a supported variant.
- `crates/bsa/tests/bsa_real.rs` — clean. The header docstring at lines 1-16 lists v103 alongside v104/v105 as a supported version.
- `ROADMAP.md:486` — clean. Explicitly notes "BSA v103 (Oblivion) decompression not working — stale premise, closed via #699". This is the spec-mandated treatment.
- `ROADMAP.md:78-79` and `:212` — clean. Both call out the `BSA v103 decompression` framing as stale and refuted by the 2026-04-17 / 2026-04-25 sweeps.
- `.claude/commands/audit-oblivion.md:19, 53-58, 80, 89` — clean. The slash-command file states "archive opens AND extracts cleanly (147,629 / 147,629 vanilla files)" and explicitly calls out the regression-guard framing.
- `docs/engine/game-compatibility.md:115-130, 209` — clean. v103 row reads `BSA v103 ✓ (was the longest-deferred archive format ... pushed to 100%)`.
- `docs/engine/archives.md:7, 30, 65, 86, 331-336` — clean. Documents v103 as supported, lists zlib for v103/v104, and the `oblivion_extract` example is described accurately as an investigation tool from the M26+ era.

No stale diagnostics to escalate.

## Findings

**No findings — Dimension 2 is materially clean.**

The carry-over from `AUDIT_OBLIVION_2026-04-25` is the only outstanding item, and it is already filed:

### Carry-over (not re-filed): `#690` — Zero on-disk v103 BSA regression test coverage

- **Severity**: MEDIUM (per 2026-04-25 audit; OPEN as of 2026-05-06)
- **Location**: `crates/bsa/tests/bsa_real.rs` (lines 54-150 cover v104 + v105 only)
- **Status**: **Existing: #690** — already tracked, not a new finding. Listed here only so the carry-over is visible in the regression-guard report; do not re-file.
- **Description**: `bsa_real.rs` ships gated integration tests for FNV (`fnv_meshes_bsa_v104_extracts_nif_with_gamebryo_magic`) and Skyrim SE (`skyrimse_meshes_bsa_v105_extracts_nif_with_gamebryo_magic`, `skyrimse_meshes_bsa_v105_brute_force_extract_zero_errors`). There is no equivalent test against a real v103 archive (e.g. `Oblivion - Meshes.bsa`). The synthetic v105 unit tests (`#617`) cover the v105-specific branches in-memory but do not touch v103 either.
- **Why this is still LOW-MEDIUM, not HIGH**: today the v103 path is empirically validated through `nif_stats` (8,032 NIFs round-tripped through the v103 zlib path) and the `oblivion_extract` example, but neither is part of `cargo test -p byroredux-bsa`. A regression in the version-dispatch arm or zlib path that broke v103 specifically (e.g. a future "simplification" that removes the `if version == 105 { 24 } else { 16 }` branch and forces 24 B everywhere) would slip through CI.
- **Suggested treatment**: leave `#690` open. The fix is a 30-line addition to `bsa_real.rs` mirroring `fnv_meshes_bsa_v104_extracts_nif_with_gamebryo_magic` against `Oblivion - Meshes.bsa` plus an env-gated `BYROREDUX_OBLIVION_DATA` data-dir helper. Out of scope for a pure regression guard; tracked for the next batch that touches `bsa_real.rs`.

## Stale-framing risk — read before re-auditing

The pre-2026-04-17 framing of "BSA v103 is broken / decompression doesn't work" is dead. It was closed via `#699` after the 2026-04-17 sweep confirmed 147,629 / 147,629 vanilla extractions clean across all 17 Oblivion BSAs and the 2026-04-25 follow-up re-verified all 12 spec-correctness invariants at then-current line numbers. This 2026-05-06 audit re-verifies invariants 1-6 at current line numbers + adds a live-smoke validation (invariant 8).

**Anyone re-reading this report should NOT add a "v103 is broken" finding without measurement.** Specifically:

1. Do NOT propose v103 fixes inferred from "the comments look risky" / "this branch handles v105 specially, maybe v103 needs the same" / "the test surface looks thin so v103 must be flaky." Per `feedback_audit_findings.md`, ~16% of audit findings are stale-premise; per `feedback_no_guessing.md`, every claim must tie to a file:line + measured failure.
2. The real Oblivion exterior blocker is **TES4 worldspace + LAND wiring**, not BSA v103 decompression. This is the same shape FO3 was in pre-cell-loader era, and the cell loader for it is tracked under M40 / M41 / "exterior renderer" Tier-1/2 plans. There is no "BSA v103 cell loader fix" deliverable.
3. If a future audit dimension finds a real v103 regression, the signal will be either:
   - `cargo test -p byroredux-bsa` red on a previously-green test, OR
   - `cargo run -p byroredux-nif --example nif_stats -- --bsa "Oblivion - Meshes.bsa"` parse rate dropping below the 96.24% baseline, OR
   - `oblivion_extract` returning a payload whose first 20 bytes are not `Gamebryo File Format` on a known-clean archive.
   None of those signals fired this audit cycle.

The right response to "I think v103 might be broken" is: run one of the three measurements above. If they all pass, the audit's job is done — close with a PASS. That is the conclusion of this dimension today.

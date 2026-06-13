## Finding NIF-NEW-03 — NIF Audit 2026-06-13

- **Severity**: HIGH
- **Dimension**: Stream Position + Version Handling
- **Game Affected**: Oblivion only (v10.1.0.106 and siblings, sizeless format).
- **Location**: `crates/nif/src/blocks/interpolator.rs` (NiBlendTransformInterpolator / controller-manager interpolator path, `parse` at line 283); surfaces at the subsequent `NiControllerSequence` (`crates/nif/src/blocks/controller/sequence.rs:102`) in `lib.rs::parse_nif`. Comparator surface: `version.rs` (`V10_1_0_108/110/111`).
- **Status**: NEW — validated CONFIRMED at HEAD `8d191d7d`.

## Description

An interpolator block in the `NiControllerManager` chain under/over-consumes; the next `NiControllerSequence` reads `name` length off a misaligned offset, gets garbage, trips the alloc cap, and truncates (sizeless format → no realignment). nif.xml carries a dense `since/until` cluster here: :1928-1936 NiControllerSequence string set `since=10.1.0.104 until=10.1.0.113`, :3327-3335 NiBlendInterpolator `Single Interpolator since=10.1.0.108 until=10.1.0.111` — version bands the parser cannot currently express (no `V10_1_0_108/110/111`).

## Evidence (validated)

- `interpolator.rs` `parse` at line 283; `NiControllerSequence` struct at `controller/sequence.rs:102`.
- `meshes\menus\lockpicking\pickold.nif` (v10.1.0.106): block 5 `NiBlendTransformInterpolator` reported `consumed=64`, block 6 `NiControllerSequence` then requested **4,294,934,527** bytes — DISCARDING 42 blocks. Static walk: the first real `0xFFFF7FFF` is ~640 bytes downstream of the believed block-6 start, confirming block 5 under-consumed (the sequence is the victim).

## Impact

13 of 56 truncated Oblivion scenes, concentrated in `menus/lockpicking/` (tumbler/bolt/pick animation rigs) and `marker_*` UI meshes. Drops the entire animation-controller subtree.

## Suggested Fix

Byte-audit the `NiControllerManager → NiTransformController → NiBlendTransformInterpolator → NiControllerSequence` chain against nif.xml for **v10.1.0.x** (the interpolator base and NiControllerSequence header carry version-gated fields — `Managed Controllers`, accumulation flags, weighted/cycle ordering — that shift between 10.1.x and 20.x). Shares the version-constant prerequisite with NIF-NEW-01/02. Add a v10.1.0.106 controller-chain fixture.

## Completeness Checks
- [ ] **UNSAFE**: N/A expected
- [ ] **SIBLING**: Audit the whole interpolator family (NiBlendTransform/NiBlendPoint3/NiTransformInterpolator) for the same 10.1.x band drift, not just the one that surfaced
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **CANONICAL-BOUNDARY**: N/A (parse-layer)
- [ ] **TESTS**: Add a v10.1.0.106 controller-chain fixture; regression check = `pickold.nif` parses with 0 dropped blocks

---
Source: `docs/audits/AUDIT_NIF_2026-06-13.md` · Filed by `/audit-publish` · Absorbs NIF-D3-NEW-09 + v10.1 half of NIF-D2-NEW-02

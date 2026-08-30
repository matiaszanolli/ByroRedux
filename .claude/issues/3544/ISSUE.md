# #3544: SK-D3-02: crates/facegen's .egt and .tri parsers have zero consumers anywhere in the workspace

**Source**: `docs/audits/AUDIT_SKYRIM_2026-08-30.md` — Dimension 3 (NPC Equip + FaceGen)
**Severity**: MEDIUM
**Location**: `crates/facegen/src/egt.rs`, `crates/facegen/src/tri.rs`, crate doc in `crates/facegen/src/lib.rs`

## Description

`crates/facegen` (1,394 LOC incl. tests) exports four public surfaces. Two of them —
`EgtFile` / `EgtMorph` and `TriHeader` — have **no consumer anywhere in the workspace**.

## Evidence

Consumer trace across the whole tree, outside the crate itself (re-verified 2026-08-30):

| Export | External consumers |
|---|---|
| `EgmFile` / `EgmMorph` | `byroredux/src/npc_spawn/resumable.rs` (Oblivion / FO3NV runtime-recipe track only) |
| `apply_morphs` | same |
| `half_to_f32` | `crates/nif/.../decode_half_float_tests.rs` (bit-for-bit parity test, #2599) |
| **`EgtFile` / `EgtMorph`** | **none** |
| **`TriHeader`** | **none** |

`grep -rn 'EgtFile\|EgtMorph\|TriHeader' --include='*.rs' .` outside `crates/facegen`
returns only `crates/facegen/examples/_tmp_fo3_facegen_probe.rs`, itself a throwaway audit
probe inside the crate.

`egt.rs` (234 LOC) parses the full FaceGen texture-morph table (`FREGT003`, 50 morphs ×
256×256×3) and nothing reads it. The crate's own module doc says "Phase 3c consumes the EGT
compositor output", but `resumable.rs`'s Phase 3b/3c log line covers FGGS+FGGA **geometry**
morphs only — there is no compositor. `tri.rs` (154 LOC) is a self-declared header-only stub
whose body parse is deferred to "M47-tier work", and even its header is unread. Both are
exercised solely by `crates/facegen/tests/parse_real_facegen.rs` — tested, but not used.

## Impact

Two parsers plus their tests are carried as production weight with no runtime effect, and
the crate doc asserts a Phase 3c compositor that does not exist. Beyond dead weight:
`crates/facegen` has **no other owner in this audit suite**, so an unconsumed parser here is
invisible to every other gate. The runtime-recipe games (Oblivion, FO3/FNV) need EGT for
per-NPC complexion, so this is also a real feature hole hiding behind a shipped-looking
parser.

Not a Skyrim-blocking gap — Skyrim is on the pre-baked FaceGen track and needs neither file
— but it is the crate's coverage answer.

## Suggested Fix

Either wire the EGT compositor into the runtime-recipe FaceGen path, or mark both modules
explicitly deferred in the crate doc (and correct the "Phase 3c consumes the EGT compositor
output" claim) so the next reader does not assume Phase 3c shipped.

## Related

#2599 (`half_to_f32` parity test). Filed with `tech-debt` because there is no `facegen`
domain label — see the missing-label note in the publish summary.

## Completeness Checks
- [ ] **SIBLING**: if EGT is wired, check the `.tri` header stub for the same "parsed but never read" shape
- [ ] **TESTS**: a regression test pins whichever outcome is chosen (a consumer assertion, or a doc/deferral marker the tests read)

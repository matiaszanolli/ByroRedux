# #3915: FNV-2026-09-05-D1-01: FNV region ambient music has no open tracking issue: #3787 closed with a diagnostic log only, and the open follow-up #3816 explicitly defers FNV back to that closed issue

Filed from `docs/audits/AUDIT_FNV_2026-09-05.md` (FNV-2026-09-05-D1-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `low,game:fnv,legacy-compat,audio,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3915 --json state`.

---

**Source**: `docs/audits/AUDIT_FNV_2026-09-05.md` (FNV-2026-09-05-D1-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: 1 — Cell Loading End-to-End (region ambient)
- **Location**: `byroredux/src/asset_provider/audio.rs` —
  `dispatch_region_ambient_music`'s `ONCE.call_once` diagnostic;
  `byroredux/src/components.rs` — `RegionAmbientRes`'s corrected doc
- **Status**: NEW (tracking gap; the underlying defect is last cycle's
  FNV-2026-08-30-D1-01, CLOSED as #3787)
- **Description**: #3787 was closed by correcting three doc sites and adding a
  once-only `log::info!` explaining that FNV's `RDSB`/`RDSI` target `MSET` and
  that no MSET runtime exists. The *runtime* work was deferred. #3816 —
  "decode Skyrim MUSC/MUST and Oblivion RDMD music-type enum for REGN ambient
  music" — is the open follow-up, and its own body says of the third era:
  "**FNV `RDSB`/`RDSI` — `MSET` (Media Set), already tracked separately by
  #3787**". #3787 is CLOSED. So the reference title is the one game of the three
  whose region-ambient work is tracked by nothing open.

  Two supporting facts, both measured, both contradicting text that is currently
  load-bearing:
  - #3816's background section states "Oblivion's `REGN` has no `MUSC` …
    **FNV inherits the identical enum unchanged**." **FalloutNV.esm ships zero
    `RDMD` sub-records.** FNV does not inherit that enum in its shipped data at
    all; it uses `RDSB`/`RDSI` exclusively.
  - The engine now parses `MSET` (`dispatch_misc_stub.rs`, into
    `EsmIndex::media_sets`) but nothing reads `media_sets` — a `grep` for it in
    `byroredux/src/` returns nothing. So the record type is decoded and the
    consumer half is absent, which is precisely the state that needs a tracking
    issue.
- **Evidence**: `gh issue view 3787` → `CLOSED 2026-09-03`; `gh issue view 3816`
  → OPEN, body defers FNV to #3787. ESM census of all 276 FNV `REGN`: `RDSB`
  ×44 (**44/44 → MSET**, 0 → SOUN), `RDSI` ×11 (**10/11 → MSET**, 1 unresolved
  DLC ref, 0 → SOUN), `RDMD` **×0**. `RDSD` (the SOUN-typed ambient-loop list)
  ×76, still deliberately unsurfaced.
- **Impact**: No runtime impact — the feature is already known-dead and logs
  why. The cost is process: an FNV-specific shipped-feature gap has fallen off
  the tracker while its Oblivion and Skyrim siblings stayed on it, and a future
  reader of #3816 will conclude FNV is covered when it is not.
- **Related**: #3787 (closed), #3811 (closed), #3816 (open), #3301 (open).
- **Suggested Fix**: Fold FNV into #3816's scope (rename to cover MSET as well)
  or reopen #3787 as the MSET-runtime issue, and correct the "FNV inherits the
  identical enum" sentence in #3816's body against the zero-`RDMD` measurement.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary

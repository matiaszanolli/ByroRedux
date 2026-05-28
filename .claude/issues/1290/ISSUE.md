Surfaced by the 2026-05-28 Starfield audit (`docs/audits/AUDIT_STARFIELD_2026-05-28.md` Dim 6).

## Issue

The 2026-05-18 Starfield audit's "Forward Blockers" section listed [#762](https://github.com/matiaszanolli/ByroRedux/issues/762) (Starfield `.mat` JSON parser + `MaterialProvider` integration) at the **top** of the chain:

1. **#762** — Starfield `.mat` JSON parser + `MaterialProvider` integration
2. `materialsbeta.cdb` reader — deferred under #762
3. `--sf-smoke` resolve-rate measurement
4. SF-only record types + ESM + space-cell concept

#762 closed 2026-05-24 with the binary CDB parser landed in `crates/sfmaterial/`. But the **consumer-side mapping is unwired** ([SF-D3-NEW-01](https://github.com/matiaszanolli/ByroRedux/issues/) — filed alongside this issue). The CDB parser exists; nothing reads from it.

So the current state is "parser landed, consumer not wired" — which reads on the forward-blocker chain as "almost there on materials" but is actually "the visible-result work is still ahead."

## Risk

Roadmap drift. Audits N months from now might assume Starfield is rendering with real materials based on the closed #762, when actually nothing is plumbed. The lowest-effort visible-progress milestone for Starfield (SF-D3-NEW-01) is misranked.

## Suggested fix

Re-order the forward-blocker chain in the next ROADMAP refresh / next Starfield audit:

1. **SF-D3-NEW-01** (sfmaterial → asset_provider consumer) — **actual top blocker**, closes "Starfield mesh renders with real PBR materials"
2. *(Optional)* `.mat` JSON sidecar parser — for mod authoring; deferred since vanilla content ships nothing loose
3. CRC32-flag-name reverse table — empirical sampling against known Bethesda flag-bit names (low priority; observability only)
4. `--sf-smoke` resolve-rate measurement — quantifies SF form-id resolution against the unparsed ESM corpus (decides whether SF ESM work is a fix-up patch or a from-scratch parser)
5. Starfield ESM parser — `PNDT` / `STDT` / `BIOM` / `SFBK` / `SUNP` / `GBFM` / `GBFT` records, entirely new types
6. Space-cell concept (M64-tier)
7. Procedural ship assembly (gameplay-driven, out of scope until ESM works)

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: check if other audit-skill specs (e.g., `.claude/commands/audit-fo4.md`, `audit-skyrim.md`) cite closed blockers in their forward-roadmap sections that may have similarly drifted
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A — doc / roadmap fix only

# #3150 — ESM-2026-08-20-D4-01: three `//! TEMP scratch` audit probes are committed as `crates/plugin` example targets — and 57 more sit in `crates/nif/examples/`

**Finding**: ESM-2026-08-20-D4-01
**Labels**: bug, import-pipeline, low, tech-debt
**Filed**: 2026-08-20 · `/audit-publish` · HEAD `bb0b92f2`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/3150

---

- **Severity**: LOW
- **Dimension**: ESM Dim 4 — record schema dispatch (hygiene)
- **Location**: `crates/plugin/examples/_tmp_obl_bsxrefr.rs:1`, `crates/plugin/examples/_tmp_obl_player.rs:1`, `crates/plugin/examples/_tmp_sk_lvli.rs:1` (added by `19e53dd8`, an `/audit-ui` report commit); plus 57 siblings in `crates/nif/examples/`
- **Status**: NEW

## Description

Three example binaries in `crates/plugin/examples/` open with a line that declares them temporary:

```rust
//! TEMP scratch (audit 2026-08-16): how many Oblivion.esm REFR placements
//! point at a base record whose model is one of the BSXFlags-bit-5 NIFs the
//! cell loader drops wholesale?
```

```rust
//! TEMP scratch (audit 2026-08-16): which NPC_ FormID is the player base
//! record on each game's master? Probes 0x07 and 0x14.
```

```rust
//! TEMP scratch (audit 2026-08-16): Skyrim LVLI LVLF flag distribution over the
//! outfit (OTFT) reachable set, to test `expand_leveled_form_id`'s
//! `flags & 0x02 => multi-pick` rule against TES5's real "Use All" bit (0x04).
```

They are committed workspace build targets: `cargo build --examples` and `cargo test` compile them. They are also the exact artefact the 2026-08-16 ESM pass went out of its way to remove, closing with *"Working tree carries no artefact of this audit outside `docs/audits/`."*

## Evidence

At HEAD `bb0b92f2`:

```
$ ls crates/plugin/examples/_tmp_* | wc -l
3
$ ls crates/nif/examples/_tmp_* | wc -l
57
$ find . -name '_tmp_*.rs' -not -path './target/*' | wc -l
70
```

The `crates/plugin/` three are the ones this audit owns and the ones dated to a specific prior audit. The larger `crates/nif/examples/` population is corroborated independently by `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-20.md` Dimension 1, which flags `crates/nif/examples/_tmp_sf_d2_*.rs` and `_tmp_*` generally as the only non-production hits in its `(x, z, -y)` swizzle sweep — i.e. this scratch population is already producing false positives in other audits' greps.

`docs/audits/AUDIT_TECH_DEBT_2026-08-20.md` from the same suite does **not** cover these, so they are otherwise unowned.

## Impact

No runtime effect. The costs are:

- **Build time and CI surface.** 70 example targets compile on `cargo test` and `cargo build --examples`.
- **Audit noise.** Tree-wide greps for production invariants (axis swizzles, `bs_version` comparisons, `GameKind` branches) hit these files and each hit has to be individually cleared. The legacy-compat sweep spent effort on exactly that this cycle.
- **Convention drift.** A `//! TEMP scratch` marker four days old that survives a session close stops being a marker.

## Related

- `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-20.md` Dim 1 ("Additional candidates checked and cleared" — the `_tmp_sf_d2_*` swizzle hits).
- `docs/audits/AUDIT_ESM_2026-08-16.md`, whose hygiene close-out this violates.

## Suggested Fix

Delete the three `crates/plugin/examples/_tmp_*.rs` files. Triage the 57 in `crates/nif/examples/` in the same change: any probe worth keeping should lose the `_tmp_` prefix and get a real doc comment stating why it is a permanent diagnostic (as `watr_wind_census.rs`, `esm_dim8_bench.rs` and `sf_smoke.rs` already do); the rest go.

Optionally add a CI check that fails on any committed path matching `_tmp_*`, so the convention is enforced rather than remembered.

---
*Filed from `docs/audits/AUDIT_ESM_2026-08-20.md` (Dim 4). Verified against HEAD `bb0b92f2` before filing — the count is 3 in `crates/plugin/` and 70 tree-wide, which is broader than the report's three.*

## Completeness Checks
- [ ] **SIBLING**: all 70 `_tmp_*.rs` files triaged, not just the three named — the `crates/nif/examples/` population is 19× larger
- [ ] **TESTS**: `cargo build --examples` and `cargo test` still succeed after removal (no non-example target imports a scratch binary)

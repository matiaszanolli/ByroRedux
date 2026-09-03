# #3788 — FNV-2026-08-30-D8-01: --game fnv supplies no --sounds-bsa, so footsteps, water splash and REGN ambient all silently early-return on the reference title

**Severity**: MEDIUM · **Location**: `assets/debug_profiles.toml` (`[profiles.fnv]`)
**Source**: `docs/audits/AUDIT_FNV_2026-08-30.md` (FNV-2026-08-30-D8-01)

`[profiles.fnv]` declared no `default_sounds_bsas`, so `--game fnv` emits zero `--sounds-bsa`
flags and all three M44 audio consumers (`try_load_default_footstep`,
`try_load_default_water_splash`, `dispatch_region_ambient_music`) silently early-return. Not a
design decision — the parser's own unit test already declares this exact shape
(`default_sounds_bsas = ["Fallout - Sound.bsa"]`), just not the shipped file. 84.7% of FNV
`SOUN.FNAM` paths (2,700/3,189) resolve inside `Fallout - Sound.bsa`, including the canonical
footstep/splash paths byte-for-byte.

## Fix implemented

1. Added `default_sounds_bsas = ["Fallout - Sound.bsa"]` to `[profiles.fnv]`.
   `Fallout - Voices1.bsa` deliberately NOT added (item 2 of the suggested fix): 105,517
   entries, no dialogue consumer exists yet.
2. Item 4 (log-once distinguishing "no archive" from "not implemented"), applied to the two
   one-shot boot-time loaders: `try_load_default_footstep`/`try_load_default_water_splash`
   (`byroredux/src/asset_provider/texture.rs`) now `log::info!` on the `sounds.is_empty()` early
   return. `dispatch_region_ambient_music` deliberately left as-is — it fires per-region
   transition (not once at boot), its own doc comment already explains the no-archive silence
   as intentional common-case behavior, and adding a per-call log there would need one-shot
   bookkeeping to avoid spam, which is out of this issue's scope.

**SIBLING** (issue's own checklist item, checked not fixed — item 3 of the suggested fix
explicitly scoped this "out of this dimension's remit"): `[profiles.fo3]` and
`[profiles.oblivion]` have the identical gap (no `default_sounds_bsas` line). Verified against
the mounted installs:
- FO3's `Fallout - Sound.bsa` carries the exact canonical footstep path byte-for-byte — an
  equally strong candidate for the same fix.
- Oblivion's `Oblivion - Sounds.bsa` does **not** carry that path (different content-authoring
  era) — extending there isn't a copy-paste win and needs its own per-game verification.

Neither profile was changed in this commit; both are candidates for a follow-up issue.

**TESTS** (issue's own checklist item): added `fnv_profile_declares_a_sounds_archive`
(`crates/game-detect/src/lib.rs`, alongside the sibling `Update.bsa` test from #3790), reading
the real shipped file and asserting `default_sounds_bsas` is non-empty and lists
`Fallout - Sound.bsa`. Verified live: removing the line makes the test fail with the correct
diagnostic; restored and re-confirmed passing. Scoped to the `fnv` profile specifically — the
issue's "every shipped profile with an audio consumer" framing is broader than what the SIBLING
finding above can currently support (Oblivion's negative result means a blanket assertion would
be premature).

Full workspace: `cargo test --no-fail-fast` 7037 passing, 0 failing.

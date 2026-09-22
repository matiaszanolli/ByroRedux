=== #4668 ===
# null: PAR-D5-2026-09-21-04: FaceGen EGT and TRI parsers mis-describe their formats (latent: no consumer) [OPEN]

## Description
- **EGT** (`crates/facegen/src/egt.rs:13-27`, `:142-151`). The FaceGen SDK manual gives each mode as `float s, <image> r, <image> g, <image> b` with `<image> = (signed char * R) * C`: planar and signed. The parser instead pushes interleaved `[bytes[o], bytes[o+1], bytes[o+2]]` triples and its own doc comment describes an offset-128 unsigned reading. The SDK header order is R, C, S, A, Texture Basis Version (vanilla 256, 256, 50, 0, 81); the parser calls A and the basis version `unknown_a`/`unknown_b` and sizes the file from S only.
- **TRI** (`crates/facegen/src/tri.rs:17-36`, `:80-98`). The SDK header order is V, T, Q, LV, LS, X, ext, Md, Ms, K. `TriHeader` stores X as `num_modifier_vertices`, ext as `num_modifiers`, Md as `num_uv_coords` and Ms as `num_quads`; Q, LV, LS and K become unknown words. Vanilla `headhuman.tri` words: 1211, 2294, 0, 0, 0, 1211, 1, 38, 8, 238. The module doc and the code also disagree with each other on the field order.

Verified unchanged at HEAD `ee6d3fb39`: `egt.rs`'s pixel loop is still interleaved `[bytes[offset], bytes[offset+1], bytes[offset+2]]`; `tri.rs`'s field assignment order is unchanged.

## Evidence
Probe `egt-stats`, vanilla `headhuman.egt`, first 10 modes, first third read as `i8`:
- lag-1 correlation 0.993 > lag-3 0.952 (planar; interleaving would make lag 3 the same-channel neighbour instead);
- lag-256 (next row) 0.972 > lag-768 0.840;
- 58.7% of bytes are within 16 of 0x00/0xFF versus 0.8% near 0x80, i.e. zero-centred signed chars, not offset-128 unsigned.

## Impact
Latent — there is no consumer (#3544) today — but the future FGTS compositor and lip-sync `.tri` work would inherit wrong decodes and misnamed fields if built directly on the current code without re-deriving the format.

## Related
#3544 (the no-consumer tracker for both formats), PAR-D5-2026-09-21-01 (sibling FaceGen finding, same crate — the live EGM decode bug)

## Suggested Fix
Decode EGT as planar `i8` planes (R*C each) and rename the TRI header fields to the SDK order. Add the vanilla header words as a real-data assertion.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D5-2026-09-21-04)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4669 ===
# null: PAR-D5-2026-09-21-05: UVD docs disagree, including a stale skill line and a field doc that contradicts its own module [OPEN]

## Description
`crates/bsa/src/uvd.rs:157-164`: `UvdHeader::bounds_min`'s doc comment says the box is "Not quantised ... a tight content bound, not a grid-aligned cell volume." The module doc (`:79-101`, dated 2026-09-15) and `exterior_cell_grid()` in the same file establish that exteriors **are** a grid-aligned 3x3 block (1,095/1,095 measured) — the field doc contradicts the module doc it sits inside.

Separately, `byroredux/src/cell_loader/precombined.rs:50-73` says no consumer exists for UVD yet, and `parse_uvd_header` has no non-example caller (`grep -rn parse_uvd_header byroredux/src crates` shows only `crates/bsa/examples/probe_uvd_corpus.rs`). Yet `.claude/commands/audit-parsers/SKILL.md` (Dim 5) still says "UVD: envelope only, consumed in `cell_loader/precombined.rs`" — also stale.

Verified unchanged at HEAD `ee6d3fb39`: `bounds_min`'s doc comment still reads the same contradicting text.

## Evidence
`grep -rn parse_uvd_header byroredux/src crates` returns only the example probe, confirming no production consumer exists yet.

## Impact
Misleading docs for the future previs consumer (#3810) and for the next audit run reading this field's contract.

## Related
#3810 (the UVD-consumer research spike this format feeds)

## Suggested Fix
Fix the field doc to match the module doc (exterior vs interior quantisation). Correct the skill bullet to "envelope only, no consumer yet".

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D5-2026-09-21-05)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
=== #4670 ===
# null: PAR-D6-2026-09-21-01: BA2 never checks name_table_offset against the file length, so a truncated mod archive fails with a bare "failed to fill whole buffer" [OPEN]

## Description
`crates/bsa/src/ba2.rs:194` and `:301`: `reader.seek(SeekFrom::Start(name_table_offset))?` is followed directly by `read_exact` with no `> file_len` check anywhere in between. The BSA sibling names the equivalent field before seeking (`archive/open.rs:110-131`, #3368) so its own error at least identifies which offset was bad; the BA2 path does not.

Verified unchanged at HEAD `ee6d3fb39`: the seek-then-read_exact sequence at both cited lines still has no bounds check ahead of it.

## Evidence
Probe `dup-scan` over the installed FO4 Data: `cuwp - textures.ba2` (BTDX v1 DX10, 338 files) declares `name_table_offset` 1,250,980,735 in a 214,135,265-byte file, which looks like a truncated download. `Ba2Archive::open` -> `Err("failed to fill whole buffer")`, surfaced as "BA2 '<path>': failed to fill whole buffer" with no offset/size context.

## Impact
Correct rejection, but an uninformative error — the operator cannot tell a truncated archive from a reader bug from the message alone.

## Related
#3368 (the BSA-side equivalent check this should mirror), PAR-D1-2026-09-21-07 (sibling I/O-discipline finding)

## Suggested Fix
Validate `name_table_offset <= file_len` (and record offset plus size) at open, with a named error mirroring #3368.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D6-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4671 ===
# null: PAR-D6-2026-09-21-02: Duplicate names inside one archive overwrite silently, and BSA keys are not separator-normalised at open [OPEN]

## Description
`crates/bsa/src/archive/open.rs:361-369` and `crates/bsa/src/ba2.rs:323-326`: both `HashMap::insert` call sites are last-wins with no log line on a collision.

Separately, `crates/bsa/src/archive/open.rs:252` only `.to_lowercase()`s BSA folder names at open time; BA2 keys go through `normalize_path` (`ba2.rs:309`) but BSA keys do not get the equivalent separator normalisation. A third-party BSA storing `/` in a folder name would produce keys that no normalised query can reach.

Verified unchanged at HEAD `ee6d3fb39`: both inserts are still unconditional `HashMap::insert` with no duplicate-count tracking, and BSA folder-name normalisation is still lowercase-only.

## Evidence
Probe `dup-scan`: 441 installed archives across all eight titles (vanilla plus installed mods) — 0 with declared count != distinct keys, i.e. hygiene-only on current content.

## Impact
No observed impact on current content; latent hygiene gap for a hand-crafted or unusually-authored third-party archive.

## Related
#3637 (the shadow-count logging precedent this should follow)

## Suggested Fix
Count overwritten keys at open and log once per archive, and apply `normalize_path` to BSA keys at open to match BA2.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D6-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4672 ===
# null: PAR-D6-2026-09-21-03: BGSM/BGEM strings decode as strict UTF-8, so one non-UTF-8 byte in any path drops the whole material [OPEN]

## Description
`crates/bgsm/src/reader.rs:94-98`: `String::from_utf8(..)` -> `Error::InvalidString` fails the whole parse on one bad byte anywhere in any string field, and the material falls back to NIF defaults with a warning. The reference Material-Editor implementation reads with .NET `BinaryReader.ReadChars` under a replacement-fallback UTF-8 decoder (`BaseMaterialFile.cs:326-336`), which is lossy rather than strict. This repo's own BSA/BA2 name tables already decode lossily, so a lossy BGSM string could still match the archive key that named it.

Verified unchanged at HEAD `ee6d3fb39`: `reader.rs:94-98` still uses strict `String::from_utf8`.

## Evidence
Vanilla: 0 of 36,888 FO4/FO76 materials hit `InvalidString` (probe `bgsm-scan`) — mod-content only impact.

## Impact
Mod-content only. Every texture slot of an otherwise-valid material is lost over a single non-UTF-8 byte anywhere in its string fields.

## Related
PAR-D6-2026-09-21-02 (sibling I/O-discipline finding, same dimension)

## Suggested Fix
Decode with `from_utf8_lossy` (matching the reference implementation and this workspace's own archive readers) and warn once per file when replacement occurred.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D6-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4673 ===
# null: PAR-D6-2026-09-21-04: game-detect has unbounded VDF recursion and joins ACF installdir without containment, and unreadable manifests vanish silently [OPEN]

## Description
`crates/game-detect/src/vdf.rs:107-159` (`Cursor::parse_entries`) recurses once per `{` block with no depth counter anywhere in the function (`grep -n depth crates/game-detect/src/vdf.rs` returns nothing).

`crates/game-detect/src/steam.rs:157-158`: `steamapps.join("common").join(install_dir)` — an absolute `installdir` value replaces the base path entirely rather than merely escaping it via `..`, and only `is_dir()` gates the result before it is reported as a detected install.

`crates/game-detect/src/steam.rs:82` and `:139`: `let Ok(text) = std::fs::read_to_string(..) else { continue }` drops an unreadable or non-UTF-8 manifest with no log line, while a genuine VDF *parse* error two lines later does `log::warn!`.

Verified unchanged at HEAD `ee6d3fb39`: no `depth` identifier exists anywhere in `vdf.rs`; both `steam.rs` read sites still silently `continue` on a read failure.

## Evidence
See the cited locations. The skill's own Dim 6 notes already record the recursion and the join as Steam-written and LOW severity.

## Impact
Requires a tampered Steam install to trigger. The launcher could report an install outside the intended library, or crash on a pathological VDF nesting depth.

## Related
`/audit-tooling` Dim 5 (policy owner for launcher/install-detect surfaces)

## Suggested Fix
Add a depth cap (for example 32) to `parse_entries`, and reject `installdir` values that are absolute or contain `..` components. Log a debug line on manifest read failures to match the parse-error path.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D6-2026-09-21-04)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4674 ===
# null: CHAR-2026-09-21-D4-01: Player populated through the NPC path — PlayerOnly ruleset rows never evaluated (FO4 Health/AP wrong, FO3/FNV AP missing) [OPEN]

**Severity**: MEDIUM
**Dimension**: Population Boundary
**Game**: FO4 (wrong values); FO3/FNV (missing AP); all (structural)

## Description

`build_player_character_template` (`byroredux/src/inventory.rs`) derives the player's `ActorValues` with `derive_npc_actor_values(player_npc, index)` — the NPC population function. For the player, several ruleset rows are `PlayerOnly` derived formulas: FO3/FNV/FO4 Health and AP, Skyrim Light Armor, and Oblivion's pools. The capture documents state the player's live values come from those formulas and NPCs ship baked values. The code instead gives the player the NPC answer, and nothing ever evaluates the `PlayerOnly` rows for the player:
- `GetActorValue` (`crates/scripting/src/condition.rs`) excludes `PlayerOnly` for every entity. Its comment still says "the player isn't modelled yet".
- `vitals_snapshot` (`byroredux/src/inventory.rs`) reads carried values only.
- No other consumer asks.

Real-master results (read-only probe of Player `NPC_` `0x00000007`):
- **FO4** (`Fallout4.esm`: PRPS SPECIAL 1×7, ACBS level 1): PRPS also authors `Health=40.0` and `ActionPoints=0.0`, and DNAM authors calc_health **150** and calc_ap **100**. The DNAM pairs are pushed after PRPS, and `from_pairs` is last-write-wins, so the player carries Health **150** and AP **100**. The capture's player formulas give **85** and **70** (1.76x / 1.43x off).
- **FO3/FNV** (auto-calc off, `PlayerClass` ATTR 5x7, level 1): Health 200 matches the capture only because the NPC curve equals the player formula at L1/END5. `ActionPoints` is never seeded.

## Evidence

Verified at HEAD `ee6d3fb39`. The two FO4 values come straight from `crates/plugin/src/esm/records/actor_value_derive.rs`:
```rust
out.extend_from_slice(props);                    // PRPS: (Health, 40.0), (ActionPoints, 0.0), …
for (avif_editor_id, baked) in [("Health", …calculated_health), ("ActionPoints", …)] {
    if baked > 0 { … out.push((fid, f32::from(baked))); }   // (Health, 150.0), (ActionPoints, 100.0)
}
```
The FO3/FNV AP result comes from `crates/scripting/src/condition.rs`'s `GetActorValue` arm: the carried fast path misses, the AP row is `PlayerOnly`, only `ActorGeneral && Absolute` rows route through `CharacterRuleset::derived_value`, and the function returns `0.0`.

## Impact

- **FO4**: the carried values win in `GetActorValue`, the native HUD, `combat_damage_system` (the player's Health pool now takes NPC `StartCombat` strikes) and drowning. A temporary END change cannot rescale HP, which the capture requires.
- **FO3/FNV**: `player.GetActorValue ActionPoints` reads 0.0 (should be 80 FNV / 75 FO3). The native HUD's AP bar never draws. `install_catalog_resolves_per_game_vital_keys` asserts only that the AP *key* resolves, and no test composes a production-stamped player.
- Latent: `consume_item` rejects a whole item when any effect's AV is not carried, and the plugin maps FO3/FNV MGEF AV 12 -> `ActionPoints`. Any AP-restoring ingestible would therefore be unusable by the player. No vanilla item in today's supported restorative set targets AP.

## Related

#4458 (the fix this follows from — correctly fixed, the stamp is live), #2937 (FO3/FNV AP scope), #4452 (the other `DerivedScope` consumer-contract gap), CHAR-2026-09-21-D5-01 (the docs that hide this), CHAR-2026-09-21-D4-03 (the rest of the half-populated player), CHAR-2026-09-21-D4-02 (same PRPS/DNAM collision, NPC-wide). Related but structurally distinct from `AUDIT_SCRIPTING_2026-09-22.md`'s SCR-D3-2026-09-22-01 (`resolve_entity_by_global_form_id` can never resolve the player by its `0x14` FormID — an entity-*resolution* gap; this finding is a formula-*evaluation* gap for an entity that already resolves correctly). Both stand independently.

## Suggested Fix

Seed only SPECIAL/skills (and FO3/FNV body conditions) from the Player record. Then give the player its `PlayerOnly` rows: evaluate them for `PlayerEntity` when stamping, or let `GetActorValue`/`vitals_snapshot` fall through to `derived_value` for the player. Drop the NPC-baked Health/AP from the player seed. Add a real-master leg asserting FO4 Health 85 / AP 70 and FNV AP 80.

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D4-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

=== #4675 ===
# null: CHAR-2026-09-21-D1-01: Vanilla --hud bars key on fallout.rs's synthetic test FormIDs (0x2C9/0x2D0) — FO3/FNV/FO4 bars never move [OPEN]

**Severity**: MEDIUM
**Dimension**: Ruleset Seam
**Game**: FO3 / FNV (MenuXml `--hud`), FO4 (Scaleform `--hud`), Skyrim (actor-identity half only)

## Description

AVIF identities are AUTHORED and must be resolved per load (CHARAL doctrine; `EsmIndex::actor_value_form_id`). The FO3/FNV MenuXml and FO4 Scaleform HUD profiles instead hardcode ids copied from a unit-test fixture. Separately, `fraction()` (`byroredux/src/hud.rs`) has two problems:
- It scans `world.query::<ActorValues>()` and returns the first entity in `SparseSetStorage`'s dense order (insertion order, perturbed by swap-remove) that carries the key. It never reads `PlayerEntity`.
- That "any stamped actor" fallback predates `eb3784309` and is obsolete now that the player carries `ActorValues`.

The native vitals path one module over (`inventory.rs`'s `build_player_vitals` + `vitals_snapshot`) already does both things correctly: it resolves keys through `actor_value_form_id` and reads the player.

## Evidence

Verified at HEAD `ee6d3fb39`. `hud.rs`'s `FALLOUT_BARS`:
```rust
static FALLOUT_BARS: &[HudBar] = &[
    HudBar { label: "hp", av: Some(0x2C9) },
    HudBar { label: "ap", av: Some(0x2D0) },
];
```
`byroredux/src/scaleform_hud.rs` mirrors this with `ScaleformBar { label: "health", av: 0x2C9 }` / `{ label: "ap", av: 0x2D0 }`, commented "FO4 keys per `crates/core/src/character/fallout.rs`". Those two ids are the FormIDs of the test-only `fn full()` resolver in `crates/core/src/character/fallout.rs` (`Health => 0x2C9`, `ActionPoints => 0x2D0`, beside `Strength => 0x05`), not any real AVIF.

Real-master AVIF tables (read-only probe, per the audit report): FNV/FO3 `AVHealth 0x450`, `AVActionPoints 0x44C`, and neither `0x2C9` nor `0x2D0` is an AVIF; FO4 `Health 0x2D4`, `ActionPoints 0x2D5`, **`0x2C9 = Experience`**, `0x2D0` = none; Skyrim `0x3E8/0x3E9/0x3EA` = AVHealth/AVMagicka/AVStamina (correct).

`fraction()` falls back to `1.0` when no entity carries the key, and otherwise takes the first hit from an unordered `world.query::<ActorValues>()` scan — never `PlayerEntity`.

## Impact

- On real FO3/FNV/FO4 content no entity carries these keys, so the HP/AP bars always draw full, regardless of damage. On FO4 the "health" bar is keyed on the `Experience` AV.
- On Skyrim the keys are right, but the bar can track an NPC's health rather than the player's.
- The smoke gates pin bar fractions via `hud.values`, so none of them catches it.

## Related

CHAR-2026-09-21-D1-02, CHAR-2026-09-21-D4-01 (the player's `ActorValues` this fix should read); PERF-D1-2026-09-21-03 and #3429 (HUD cost, not keys). `AUDIT_UI_2026-09-21.md` (Dim/§4, "existing findings re-confirmed") independently confirmed this exact bug and explicitly assigns the filing to `/audit-character`, adding three UI-side extensions: the same wrong keys are stated as fact in `docs/engine/ui.md:855-858,957-958,865-867`, the ROADMAP M48.5/M48.7 rows and `hud.rs:43-45`/`scaleform_hud.rs:83-84`; `hud.status` prints a constant `1.00 (auto)` for every unpinned bar so no MenuXml observable reports the live-derived fraction; and both drivers call `fraction()` before the change-signature check every frame, so the full-storage `ActorValues` scan runs per bar per frame even on a static HUD (reading `PlayerEntity` removes that cost too).

## Suggested Fix

Resolve each bar's key once per load via `index.actor_value_form_id(editor_id)`, reusing `build_player_vitals`'s resolution. Read `PlayerEntity`'s `ActorValues` rather than scanning all actors. Delete the literal ids and the doc claim at `hud.rs:43-45`/`scaleform_hud.rs:83-84`, and the stale claims in `docs/engine/ui.md` and the ROADMAP M48.5/M48.7 rows. Fix `hud.status` to print the live fraction. The fix spans `/audit-character` (this issue) and `/audit-ui` (`hud.rs`/`scaleform_hud.rs` owners, doc sites, and the per-frame scan cost).

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D1-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

=== #4676 ===
# null: CHAR-2026-09-21-D5-01: Doc rot — four CHARAL sites still say no stat-bearing player actor exists, after #4458 made one [OPEN]

**Severity**: MEDIUM
**Dimension**: Coverage & Doctrine
**Game**: all (FO4 + FO3/FNV most directly)

## Description

`eb3784309` (#4458) wired a stat-bearing player, but none of the CHARAL prose that states the opposite was updated. The two FO4 capture caveats are exactly what a contributor fixing CHAR-2026-09-21-D4-01 would read: they say the player formulas have nowhere to apply and application is deferred, while the code has already applied the NPC path instead. `mod_docstring_indexes_every_sub_module` checks module names only, so nothing catches this.

## Evidence

Verified at HEAD `ee6d3fb39`, all four sites still present, unchanged since `eb3784309` (2026-09-19):
- `docs/engine/charal.md` §7: "No player chargen yet. There is still no stat-bearing player-actor entity (`scene.rs`'s `player_entity` is an `AnimationPlayer`)".
- `docs/engine/charal-fo4-ruleset.md`, Health caveat 2: "No player-actor entity yet ... application deferred"; and the AP "Application caveat" making the same claim.
- `crates/scripting/src/condition.rs`, `GetActorValue`: "NPCs bake them, the player isn't modelled yet".
- `docs/feature-matrix.md`'s CHARAL table has an NPC-population row and no player-seed row.

The player seed has existed since 2026-09-19 (`eb3784309`); all four sites predate it and were not touched by it.

## Impact

- The capture documents, the authority for this layer, misstate the player's state in a way that steers the next change away from the real defect (CHAR-2026-09-21-D4-01).
- The feature matrix gives no signal that a player seed exists, or which games get it.

## Related

CHAR-2026-09-21-D4-01, #4458; sibling doc-rot clusters #4459, #4460 (still open, same class of drift).

## Suggested Fix

Rewrite the three doc sites to say the player carries a seed from the base Player `NPC_` via `derive_npc_actor_values` (#4458), and that the player-only formulas are not yet evaluated for it (CHAR-2026-09-21-D4-01). Fix the `condition.rs` comment. Add a "Player actor-value seed" row to feature-matrix's CHARAL table (FO3/FNV/FO4/Skyrim yes, Oblivion no, FO76/Starfield partial).

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D5-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)

=== #4677 ===
# null: CHAR-2026-09-21-D4-02: FO4 stored path emits duplicate Health/AP keys on ~2,500 vanilla NPCs — DNAM-wins precedence holds only by push order [OPEN]

**Severity**: LOW
**Dimension**: Population Boundary
**Game**: FO4 (and FO76/Starfield via the same `Stored` model)

## Description

`derive_stored_actor_values` (`crates/plugin/src/esm/records/actor_value_derive.rs`) copies PRPS verbatim and then pushes the baked DNAM Health/AP. The same AVIF keys therefore appear twice, and `ActorValues::from_pairs` (`set_base` in order, `crates/core/src/ecs/components/actor_values.rs`) keeps the last one. The result matches the capture only because the DNAM pushes come second:
- Nothing states this precedence. The function doc says "PRPS verbatim plus baked DNAM", and the capture never mentions PRPS authoring these keys.
- No test covers the collision: `fo4_stored_returns_prps_verbatim_plus_baked_derived`'s PRPS fixture has no Health/AP pair.

## Evidence

Verified at HEAD `ee6d3fb39`: `out.extend_from_slice(props);` precedes the `for (avif_editor_id, baked) in [("Health", …), ("ActionPoints", …)]` push loop.

Census of `Fallout4.esm` (read-only, per the audit report) covers 3,015 `NPC_` records:
- PRPS authors Health on 2,848. Of those, 2,525 also carry DNAM calc_health > 0, and 2,490 disagree.
- PRPS authors ActionPoints on 2,800. Of those, 2,415 carry DNAM AP > 0, and 799 disagree.
- Colliding PRPS Health values: 100.0 x611, 0.0 x350, **-10.0 x185**, 50.0 x170, 40.0 x131.

## Impact

- Correct today.
- A reorder, sort-by-key or dedup-first refactor would silently give ~1,000 vanilla FO4 actors 0 or -10 base Health, which means dead on spawn or undamageable.

## Related

CHAR-2026-09-21-D4-01 (the player sees the same collision, where even the DNAM answer is wrong); #3481 / #4086 (earlier FO4 stored-path precedence fixes).

## Suggested Fix

- Drop PRPS pairs keyed on the Health/AP AVIFs when a baked value is present, so the precedence is explicit.
- Add a fixture with PRPS Health -10 and DNAM 150, asserting 150.
- Record the census in the FO4 capture's NPC-storage section.

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D4-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix


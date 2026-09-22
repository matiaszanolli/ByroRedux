=== #4646 ===
# null: ESM-2026-09-21-D4-02: two doc comments on the creature-routing path still say placed creatures are "Oblivion ACRE, ACHR→CREA from FO3 on" [OPEN]

## Description
Two doc comments on the creature-routing path still state the premise that CLOSED #3755 showed to be false: that placed creatures are "Oblivion `ACRE`, `ACHR`→`CREA` from FO3 on" — i.e. that `ACRE` is Oblivion-exclusive. #3755 corrected the `crates/plugin/src/esm/cell/walkers.rs` comment, but two sibling comments dating from the same commit (`ec21c1f2a`, 2026-08-14) were not updated.

## Evidence
Current code:
- `crates/plugin/src/esm/records/index.rs:520-525` (`EsmIndex::actor` doc): "... a placed `ACRE` (Oblivion) or `ACHR`→`CREA` (FO3+) fell through to the generic static-mesh path ...".
- `byroredux/src/cell_loader/references/mod.rs:639-643`: "... every placed creature (Oblivion `ACRE`, and `ACHR`→`CREA` from FO3 on) missed the actor pipeline entirely ...".

Real masters contradict the "Oblivion-only ACRE" framing: `Fallout3.esm` has 3,349 `ACRE` and 2,154 `ACHR`; `FalloutNV.esm` has 2,999 `ACRE` and 3,386 `ACHR` — both record types coexist on FO3/FNV, they are not partitioned by game.

## Impact
None at runtime — routing goes by base record type through `EsmIndex::actor`, which already handles both `NPC_` and `CREA` correctly regardless of which placement record (`ACRE`/`ACHR`) points at them. The comments are a documentation-only hazard for future edits that might reintroduce a game-gated `ACRE` check on the strength of this wording.

## Related
#3755, #2567 (CLOSED — #3755 fixed the `cell/walkers.rs` comment; these two are its siblings)

## Suggested Fix
Reword both comments to "placed `ACRE` (Oblivion/FO3/FNV) or `ACHR`→`CREA`", matching the corrected `cell/walkers.rs` wording.

Source: docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D4-02)

## Completeness Checks
- [ ] **TESTS**: comment-only fix — no behavioral test needed

=== #4647 ===
# null: ESM-2026-09-21-D7-01: two references to the deleted crates/plugin/src/legacy/ module survived #4384 [OPEN]

## Description
Two references to the deleted `crates/plugin/src/legacy/` module survived CLOSED #4384's deletion (`e3131f5ef`). `docs/engine/plugin-loading.md` was updated at the time; these two references were not.

## Evidence
- `crates/plugin/src/esm/reader.rs:385-386`: the public `FormIdRemap` doc says "See `FormIdPair` in `crates/plugin/src/legacy/mod.rs` for the dual-index form."
- `byroredux/src/sf_smoke.rs:317`: the low-resolve-rate diagnostic branch prints "... Milestone B will need a `crates/plugin/src/legacy/starfield.rs` from-scratch parser, not a delta on FO4."
- `ls crates/plugin/src` confirms there is no `legacy/` directory today. `FormIdPair` actually lives at `crates/core/src/form_id.rs:123`.

## Impact
Documentation and diagnostic-text rot only — no functional effect. A developer following either reference lands on a nonexistent path.

## Related
#4384 (CLOSED — deleted the `legacy/` module), #1322 (CLOSED)

## Suggested Fix
Point the `FormIdRemap` doc comment at `byroredux_core::form_id::FormIdPair`, and update the `sf_smoke` diagnostic hint to name the live `crates/plugin/src/esm/records/dispatch_*.rs` tier instead of the deleted `legacy/starfield.rs` path.

Source: docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D7-01)

## Completeness Checks
- [ ] **TESTS**: comment/diagnostic-text-only fix — no behavioral test needed

=== #4648 ===
# null: PAR-D1-2026-09-21-01: HKX materialises one on-disk string once per pointer to it: memory is quadratic in file size, with no cap [OPEN]

## Description
`crates/hkx/src/packfile.rs:172-193` (`Packfile::parse`'s virtual-fixup loop), `crates/hkx/src/animation.rs:290-307` (`decode_skeleton`'s bone-name loop) and `crates/hkx/src/animation.rs:518-577` (`read_annotations`) each materialise one owned `String` copy per pointer to a shared on-disk string, with no length cap and no de-duplication.

- **Virtual fixups.** `Packfile::parse` turns every virtual-fixup entry into `(offset, read_cstr(..).to_owned())`. The entry count is bounded only by the table size, which is file-sized.
- **Bone names.** `decode_skeleton` copies each bone name via `.to_owned()`.
- **Annotations.** `read_annotations` copies each annotation text and clones the track name per annotation.
- The existing count caps (4,096 bones, 65,536 annotations, `MAX_TRANSFORM_SAMPLES` = 16,000,000 samples) bound the number of *objects*, not the bytes each one owns. N pointers to one M-byte shared string cost N·M bytes of real (written) memory, and `read_cstr` also rescans the M bytes on every pointer.

Verified unchanged at HEAD `ee6d3fb39`: no string-length cap, no dedup, in any of the three sites.

## Evidence
Probe results (`hkx-bones` / `hkx-vfix`, builder follows `packfile::fixtures::PackfileBuilder`'s 64-bit layout):

```
hkaSkeleton file 368,929 B (4096 bones -> one 65,536 B name): decode Ok in 0.80 s;
    owned name bytes 268,435,456; VmHWM 3,916 kB -> 266,544 kB
packfile 256,561 B (20,000 virtual fixups -> one 16,384 B class name): decode_skeleton -> Err(MissingClass)
    after VmHWM 3,896 kB -> 322,844 kB (the copies happen inside Packfile::parse, before any class lookup)
```

With N = S/24 fixups and an S/2-byte name, a file of S bytes allocates about S²/48. That is roughly 21 GB for a 1 MB file and 83 GB for a 2 MB file. The annotation route reaches 65,536 × M.

## Impact
OOM kill or `handle_alloc_error` abort. Neither is interceptable.

- `decode_skeleton` / `decode_spline_animation` run on the main thread from `byroredux/src/asset_provider/animation.rs`: `skeleton.hkx`, the cart-idle family, `1hm_walkforward.hkx`, and the draugr rig plus three clips.
- Lookup goes through `TextureProvider::extract_mesh`, where the last-listed archive wins (#3637 precedence). A mod BSA that overrides `skeleton.hkx` (a very common kind of mod) is enough.
- Skyrim only (HKX is Skyrim's animation format in this workspace).

## Related
#3011 (sample-count bomb, closed; counts only), #4332 (layout gates), PAR-D1-2026-09-21-02 (companion Size Discipline finding, same crate)

## Suggested Fix
- Resolve virtual-fixup class names by reference (store the name offset, or compare in place against the few classes the crate looks up) instead of owning a copy per entry.
- Cap name and annotation string length (Havok names are short, e.g. 256 bytes).
- Budget total owned string bytes per decode relative to `bytes.len()`.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4649 ===
# null: PAR-D1-2026-09-21-01: HKX materialises one on-disk string once per pointer to it: memory is quadratic in file size, with no cap [OPEN]

## Description
`crates/hkx/src/packfile.rs:172-193` (`Packfile::parse`'s virtual-fixup loop), `crates/hkx/src/animation.rs:290-307` (`decode_skeleton`'s bone-name loop) and `crates/hkx/src/animation.rs:518-577` (`read_annotations`) each materialise one owned `String` copy per pointer to a shared on-disk string, with no length cap and no de-duplication.

- **Virtual fixups.** `Packfile::parse` turns every virtual-fixup entry into `(offset, read_cstr(..).to_owned())`. The entry count is bounded only by the table size, which is file-sized.
- **Bone names.** `decode_skeleton` copies each bone name via `.to_owned()`.
- **Annotations.** `read_annotations` copies each annotation text and clones the track name per annotation.
- The existing count caps (4,096 bones, 65,536 annotations, `MAX_TRANSFORM_SAMPLES` = 16,000,000 samples) bound the number of *objects*, not the bytes each one owns. N pointers to one M-byte shared string cost N·M bytes of real (written) memory, and `read_cstr` also rescans the M bytes on every pointer.

Verified unchanged at HEAD `ee6d3fb39`: no string-length cap, no dedup, in any of the three sites.

## Evidence
Probe results (`hkx-bones` / `hkx-vfix`, builder follows `packfile::fixtures::PackfileBuilder`'s 64-bit layout):

```
hkaSkeleton file 368,929 B (4096 bones -> one 65,536 B name): decode Ok in 0.80 s;
    owned name bytes 268,435,456; VmHWM 3,916 kB -> 266,544 kB
packfile 256,561 B (20,000 virtual fixups -> one 16,384 B class name): decode_skeleton -> Err(MissingClass)
    after VmHWM 3,896 kB -> 322,844 kB (the copies happen inside Packfile::parse, before any class lookup)
```

With N = S/24 fixups and an S/2-byte name, a file of S bytes allocates about S²/48. That is roughly 21 GB for a 1 MB file and 83 GB for a 2 MB file. The annotation route reaches 65,536 × M.

## Impact
OOM kill or `handle_alloc_error` abort. Neither is interceptable.

- `decode_skeleton` / `decode_spline_animation` run on the main thread from `byroredux/src/asset_provider/animation.rs`: `skeleton.hkx`, the cart-idle family, `1hm_walkforward.hkx`, and the draugr rig plus three clips.
- Lookup goes through `TextureProvider::extract_mesh`, where the last-listed archive wins (#3637 precedence). A mod BSA that overrides `skeleton.hkx` (a very common kind of mod) is enough.
- Skyrim only (HKX is Skyrim's animation format in this workspace).

## Related
#3011 (sample-count bomb, closed; counts only), #4332 (layout gates), PAR-D1-2026-09-21-02 (companion Size Discipline finding, same crate)

## Suggested Fix
- Resolve virtual-fixup class names by reference (store the name offset, or compare in place against the few classes the crate looks up) instead of owning a copy per entry.
- Cap name and annotation string length (Havok names are short, e.g. 256 bytes).
- Budget total owned string bytes per decode relative to `bytes.len()`.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4650 ===
# null: PAR-D1-2026-09-21-04: MenuXml <include> splices expand exponentially: include nesting never counts toward the depth cap [OPEN]

## Description
`crates/menuxml/src/parse.rs:638-679` (`splice_include`) re-enters `parse_element_content` (`:600-611` call site) with the **same** `depth` value it received. The tile cap (48) and op cap (64) therefore never see include nesting at all.

- `seen_includes` is an ancestor-path stack. It stops a file from including itself on the current path (`include_cycles_terminate` covers this), but a DAG of distinct fragments with fan-out f and depth d is still expanded to f^d splices — no cap exists for that shape.
- Each splice re-fetches the fragment through `MenuFileSource::menu_xml`, which is an archive extract plus inflate in production.
- Spellings are compared as normalised strings (`x.xml`, `prefabs\x.xml`, `menus\prefabs\x.xml` all resolve to the same archive entry through the candidate list). A fragment that includes itself under K spellings therefore re-enters about K! times.

Verified unchanged at HEAD `ee6d3fb39`: `splice_include` still calls `parse_element_content(&mut scanner, src, seen_includes, depth)` — `depth`, not `depth + 1`.

## Evidence
Probe `menuxml-bomb` (in-memory source; each extra level doubles both time and memory):

```
depth 10 (11 files,  481 B):     1,025 tiles,     2,047 fetches,  10 ms
depth 17 (18 files,  817 B):   131,073 tiles,   262,143 fetches, 0.77 s, VmHWM 45.5 MB
depth 19 (20 files,  913 B):   524,289 tiles, 1,048,575 fetches, 3.09 s, VmHWM 177.5 MB
```

Depth 30 is about 1.4 KB of XML. It extrapolates to about 1.07e9 tiles, roughly 360 GB, and hours of archive fetches.

## Impact
Hang, then OOM abort, while loading the HUD.

- The path is `hud.rs` `launch_hud` -> `MenuRenderer::load_with_profile` / `graft_fragment`, on the main thread with no `catch_unwind`.
- Source: the single `Oblivion - Misc.bsa` / `Fallout - Misc.bsa` beside the ESM (`hud.rs:189-214`, opt-in `--hud`).
- Mods cannot add a second menu archive, so the trigger is a modified or replaced Misc BSA.
- Vanilla content does not trigger it (`vanilla_corpus`/`fo3_corpus` pass).

## Related
PAR-D1-2026-09-21-05 (sibling MenuXml parser finding); `include_cycles_terminate` (covers cycles only, not fan-out); `AUDIT_SAFETY_2026-09-21` states "menuxml … recursion is bounded" — true for tiles/ops, not for include chains

## Suggested Fix
- Pass `depth + 1` (or a separate include depth) into the spliced `parse_element_content`.
- Keep a per-document budget on total include splices and total tiles, with a warning when truncating.
- Normalise include keys by the resolved archive key, not the authored spelling.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-04)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4651 ===
# null: PAR-D1-2026-09-21-05: MenuXml scanner panics when a non-ASCII character follows a comment, and silently drops a byte when it is ASCII [OPEN]

## Description
`crates/menuxml/src/parse.rs`: `take_text` (`:499-504` region of `parse_element_content`), `skip_trivia` (`:315-327`, comment-skip via `skip_ws`), and the two "drop one byte to guarantee progress" fallbacks (`:552-554`, `:595-598`) combine into a panic on valid UTF-8 input.

- `take_text` stops at the `<` of a comment. The following `skip_trivia` skips the comment and lands on body text. `take_element()` then returns `None` because the text does not start with `<`.
- The "drop one byte to guarantee progress" fallback (`scanner.pos += 1`) advances one **byte**. The next `self.src[self.pos..]` slice (inside `skip_ws`, line 327) panics when that byte sits inside a multi-byte character — `self.src[self.pos..]` panics on a non-char-boundary index.
- When the character is ASCII, the byte is silently lost instead, which corrupts the trait value.

Verified unchanged at HEAD `ee6d3fb39`: `skip_ws`'s `while let Some(c) = self.src[self.pos..].chars().next()` is still at the exact line (327) the original probe's panic trace names.

## Evidence
Probe `menuxml-midchar`:

```
<string>abc<!-- note -->été</string>   -> panicked at parse.rs:327:37: start byte index 55 is not a char boundary; it is inside 'é'
<rect>junk<!-- note --> über<x>1</x>  -> same panic (parse_element_content path), inside 'ü'
<string>plain<!-- note -->ascii</string> -> Str("plainscii")
```

Vanilla scan (probe `menuxml-scan`) of 309 vanilla menu XMLs (Oblivion 89, FNV 121, FO3 99): 0 `text<!--c-->text` sites. Their only non-ASCII bytes are leading UTF-8 BOMs on 6 FNV and 6 FO3 prefabs, which `take_text` consumes harmlessly. Vanilla is unaffected.

## Impact
Engine panic at `--hud` launch (main thread, no `catch_unwind`) from a modified or replaced Misc BSA, or silent value corruption with ASCII input. Same bug class as #3391 (`&str` byte-slicing of disk-derived text).

## Related
#3391 (same bug class — byte-slicing disk-derived text), PAR-D1-2026-09-21-04, PAR-D2-2026-09-21-03

## Suggested Fix
Advance by `self.src[self.pos..].chars().next().map_or(1, char::len_utf8)`, or better, consume the stray text with `take_text` so it is kept rather than dropped. Add the three probe strings as unit tests.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-05)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4652 ===
# null: PAR-D2-2026-09-21-03: MenuXml HUD load panics on a .fnt shorter than 12 bytes [OPEN]

## Description
`crates/menuxml/src/menu.rs:188` — `load_with_profile` computes the font's atlas name with `String::from_utf8_lossy(&fnt[12..])` **before** calling `Font::parse`. `Font::parse` has its own `fnt.len() < HEADER_LEN` -> `FontError::TruncatedHeader` check (`font.rs:74-76`), but it never runs because the slice panics first for any `.fnt` shorter than 12 bytes.

Owner note: `menu.rs` is `/audit-ui` territory. `/audit-ui`'s own `AUDIT_UI_2026-09-21.md` run confirmed this same panic and explicitly deferred filing it to `/audit-parsers` (this report), since it is a file-byte-reachable panic on the reader load path.

Verified unchanged at HEAD `ee6d3fb39`: `let name = String::from_utf8_lossy(&fnt[12..])` is still the first thing done with `fnt` in that closure, ahead of `Font::parse`.

## Evidence
Probe `menuxml-short-fnt` (8-byte font through the public API):

```
thread 'main' panicked at crates/menuxml/src/menu.rs:188:56:
range start index 12 out of range for slice of length 8
```

## Impact
Engine panic at `--hud` launch (main thread, no `catch_unwind`). Fonts come from the Misc BSA (Oblivion) or the texture BSA (FO3/FNV, `FontArchive::Textures`, user-selectable via `--hud-textures`). The rest of `load_with_profile` already degrades a failed font to `None` with a warning — this one site bypasses that degrade-gracefully path entirely.

## Related
PAR-D1-2026-09-21-05 (sibling MenuXml parse-path panic); co-owned with `/audit-ui`'s AUDIT_UI_2026-09-21.md, which confirmed this site without re-filing it

## Suggested Fix
Use `fnt.get(12..).unwrap_or_default()`, or read the name inside `Font::parse` after its length check, so a short font takes the existing warn-and-`None` path instead of panicking.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D2-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4653 ===
# null: PAR-D5-2026-09-21-01: FaceGen .egm morph deltas are decoded as IEEE half-floats, but the format stores per-morph-scaled signed 16-bit integers [OPEN]

## Description
`crates/facegen/src/egm.rs:144-151` decodes each delta component with `half_to_f32(u16)`. The FaceGen SDK file-format manual specifies, per mode, a `float` scale x, then for each vertex "3 signed short m. The actual morph values should be m * x". PyFFI's EGM format (scale = n/32768.0, integer components) agrees.

- `i16` and `f16` are both 2 bytes, so the exact-size check (`egm.rs:18-28`) and the real-data test both pass on wrongly-typed data — the bug is invisible to size/shape validation.
- The evaluator's rationale that "FaceGen used NaN as a 'no displacement' sentinel" (`crates/facegen/src/eval.rs:20-30`) describes a symptom of this mis-decode, not an intentional design: those "NaN" bit patterns are small negative int16s reinterpreted as half-floats.
- `egm.rs:15`'s doc comment "num_vertices verified == base head NIF's vertex count" is also false — EGM V = TRI V + K (vanilla `headhuman.tri`: V=1211, K=238 -> 1449). The consumer's "best-effort prefix" over the first 1211 (`crates/facegen/src/lib.rs:79-118`) is therefore the right base-vertex mapping; only the numeric type of the delta components is the defect.

Verified unchanged at HEAD `ee6d3fb39`: `egm.rs:147-149` still decodes `dx`/`dy`/`dz` via `half_to_f32(u16::from_le_bytes(...))`.

## Evidence
Probe `egm-stats`, vanilla FNV `meshes\characters\head\headhuman.egm`, 1449 verts, 50+30 morphs:

```
morphs whose max |int16| >= 30000: 80 of 80   (all exactly 32767: full-range int16 quantisation)
f16 non-finite: 32,456 (9.33%), of which int16 in [-1024,-1]: 32,319   (the rest are int16 31744..32767)
int16 sign split: pos 170,754 / neg 170,898 / zero 6,108
finite components 315,304: mean |true delta| 0.00727, mean |f16 decode - true| 0.01349 (1.86x); 16.4% off by > 2x
```

Same signature on FNV `cowboyhat.egm` (675 v), `hockeymask.egm` (1424 v) and Oblivion `armor\chainmail\m\helmet.egm` (779 v): 80/80 near-full-range, 7.7-13.0% NaN. Negative deltas in [-32768, -1025] decode in inverted magnitude order under the wrong interpretation: -1025 -> -65504*scale, and -32768 -> -0.

## Impact
Every runtime-FaceGen NPC head, via `npc_spawn/resumable.rs:1193-1239` -> `apply_morphs` (FO3/FNV, and Oblivion where a recipe exists), is deformed by wrong deltas. About 9% of components are dropped as NaN/Inf and the rest have nonlinear, partly inverted magnitudes. `docs/feature-matrix.md:76` lists FaceGen morphs (check) for FO3/FNV. The real-data test only prints the non-finite count, so nothing catches this.

## Related
#3048 (evaluator NaN/overflow guard), #2599 (the facegen `half_to_f32` copy exists only for this decode), #3544, PAR-D5-2026-09-21-04 (sibling FaceGen finding — EGT/TRI mislabeling, same crate)

## Suggested Fix
- Decode `i16::from_le_bytes` x morph `scale` (`EgmMorph.deltas` can then hold final displacements directly).
- Delete the facegen `half_to_f32` and its #2599 pin, and fix the format docs.
- Make `parse_vanilla_headhuman_egm` assert zero non-finite components and every morph's max |raw| near 32767.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D5-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
=== #4654 ===
# null: PAR-D5-2026-09-21-02: BGSM specular_enabled = false is never read, so specular is forwarded unconditionally [OPEN]

## Description
`byroredux/src/asset_provider/material/merge.rs:792-797` (`merge_bgsm_arm`) copies `specular_color` and `specular_mult` into `ImportedMaterial` for the first BGSM in the chain without checking `bgsm.specular_enabled`. The adjacent glossiness block (`:562-576`) also derives roughness from `smoothness` regardless of the same flag.

The NIF path honours the authoring intent this drops: `NiSpecularProperty` disabled zeroes both `specular_strength` and `specular_color` (`crates/nif/src/import/material/walker.rs:188-191`, #696), specifically so the glass-IOR branch cannot re-promote specular. `translate_material` then passes both fields straight through (`material_translate.rs:608-614`) with no BGSM-side gate to match the NIF-side one.

Verified unchanged at HEAD `ee6d3fb39`: `merge.rs:792-797` still reads
```rust
if !set_specular {
    material.specular_color = bgsm.specular_color;
    material.specular_strength = bgsm.specular_mult;
    set_specular = true;
    *touched = true;
}
```
with no `bgsm.specular_enabled` check anywhere in the function.

## Evidence
Probe `bgsm-fields` / `bgsm-specoff` over the vanilla material archives:

```
Fallout4 - Materials.ba2   : 467 BGSM specular_enabled=false, 466 of them with specular_mult > 0 (of 6,616)
SeventySix - Materials.ba2 : 653 / 612 (of 25,888)
FO4 by folder: paintingsgeneric 82, comicsandmagazines(+highres) 53, signage 31, buildings 27, grognak 25,
               interiors\building 20, vault 18, diamondcity 17, ...
typical values: specular_color [1,1,1], specular_mult 1.0, smoothness 1.0 (defaults left in place)
```

`bgsm_merge.rs` has no test with `specular_enabled: false`.

## Impact
Matte printed or painted surfaces (paintings, magazines, signage, architecture) render with full-strength white specular, and a smoothness-derived roughness they were authored to ignore. This is a divergent `Material` out of NIFAL translation (HIGH floor per `/audit-nifal`'s own severity ceiling for translation defects) on about 7% of vanilla FO4 BGSMs (and a comparable share of FO76's).

## Related
#696 (the NIF-side gate this should mirror), #220, #1454, #3639; overlaps `/audit-nifal` territory (`byroredux/src/asset_provider/material/merge.rs`) — checked against NIFAL's own two published reports (#4632-4637): none of those findings cover this specular_enabled gap, so this is filed fresh rather than as a duplicate. #4636 (NIFAL-D8, merge.rs `fill()` precedence doc) touches the same file at a different site.

## Suggested Fix
- Gate the specular forward (and the `set_specular` sentinel) on `bgsm.specular_enabled` across the chain.
- When disabled, zero `specular_strength` and `specular_color` exactly as `walker.rs` does, and leave roughness at the matte default rather than `1 - smoothness`.
- Add a `bgsm_merge.rs` test with `specular_enabled: false`.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D5-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
=== #4655 ===
# null: PAR-D1-2026-09-21-02: HKX MAX_TRANSFORM_SAMPLES is absolute: a 17 KB clip decodes to 610 MiB and is retained as about 2 GB of keys [OPEN]

## Description
`crates/hkx/src/animation.rs:52` defines `MAX_TRANSFORM_SAMPLES: usize = 16_000_000` as an absolute cap on `transform_count * num_frames` (checked at `:338-358`). #3011 bounded that product to stop allocator aborts, but the bound is absolute rather than tied to the data actually present in the file. Static tracks cost 0 bytes per frame in the on-disk encoding, so decoded output size is decoupled from file size.

`convert_hkx_clip` (`:444-452`, called from `byroredux/src/asset_provider/animation.rs:392-474`) then expands each sample into `TranslationKey` (56 B), `RotationKey` (48 B) and `ScaleKey` (32 B), about 136 B/sample on the default glam layout, and the result is registered in `AnimationClipRegistry` for the session — a persistent, not transient, cost.

Verified unchanged at HEAD `ee6d3fb39`: the constant is still `16_000_000`, and there is still no `num_frames <= num_blocks * (max_frames_per_block - 1) + 1` style relative check.

## Evidence
Probe `hkx-samples`:

```
spline clip file 16,929 B: Ok, 4096 tracks x 3906 frames = 15,998,976 samples (40 B each = 610 MB) in 1.37 s;
    VmHWM 3,376 kB -> 651,988 kB
```

99 tracks x 161,616 frames binds fully to the vanilla 99-bone skeleton. That gives about 2.18e9 bytes of converted keys per clip, on top of the transient 610 MiB decode.

## Impact
One mod-replaced clip pins about 2 GB for the session. The cart catalogue alone installs up to 16 clips, so a few such files exhaust RAM on a 16 GB machine. Main thread, Skyrim only.

## Related
#3011 (original sample-count bomb fix — this is the same bound's absoluteness, not a regression of it), PAR-D1-2026-09-21-01 (companion HKX Size Discipline finding)

## Suggested Fix
- Require `num_frames <= num_blocks * (max_frames_per_block - 1) + 1`.
- Cap `max_frames_per_block` and `num_frames` at a measured vanilla ceiling (plus headroom), so output stays proportional to the per-block data the file must actually carry.
- Optionally budget converted keys per clip.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix

# #5396: NIFAL-D8-2026-10-08-01: #4277's loose `.mat` decoder was written for an invented JSON layout, so it decodes none of the 20 real loose `.mat` files installed and now skips their CDB lookup

**Labels**: medium,nifal,import-pipeline,bug,game:starfield
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5396

**Source**: `docs/audits/AUDIT_NIFAL_2026-10-08.md` — `NIFAL-D8-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: this issue also covers PAR-D3-2026-10-08-02 from `docs/audits/AUDIT_PARSERS_2026-10-08.md` (`Index` `as u8` slot wrap at `loose_mat.rs` `parse_loose_mat`, and the 6-digit hex alpha read from the blue byte in `decode_rgba`), which was folded in here rather than filed separately — the decoder rewrite should use `u8::try_from` and default alpha to `FF`.

- **Severity**: MEDIUM. This is the "translatable data silently dropped" row: the authored loose-material textures and settings
  are never captured. The content is mod (Creation) content, not vanilla, so it is not HIGH.
- **Dimension**: Shader-flags/Effects (the merge boundary, the Starfield `.mat` arm)
- **Tier Violated**: no-fabrication (the layout is unsourced), and parked-not-leak (the translation is wired but cannot fire)
- **Game Affected**: Starfield (Creation/mod archives)
- **Location**:
  - `byroredux/src/asset_provider/material/loose_mat.rs:52-136` (`parse_loose_mat`) and `:153-182` (`decode_rgba`);
  - `byroredux/src/asset_provider/material/merge.rs:479-490` (the loose arm short-circuits before `lookup_cdb_material`);
  - `merge.rs:1683-1720` (`apply_loose_mat`);
  - `merge.rs:436-440` and `:573-575` (stale comments).
- **Status**: NEW. This is an incomplete fix of #4277, which was closed by `c2f28e06c`. It subsumes the renderer audit's routed
  note (`AUDIT_RENDERER_2026-10-08.md:287`, `Index as u8` wrap and `f64 as f32` overflow).
- **Description**: The decoder looks for a top-level `Components` array whose objects carry their properties directly:
  `File`/`Path`, a JSON-bool `Enabled`, a numeric `Value`, and an array or hex `Color`. The real files differ on every one of
  those points:
  1. **Root**: the top-level keys are `Filename`, `Import`, `Objects`, `Summary`, `Version`, and components live at
     `Objects[i].Components[j]`. `find_array(&value, &["Components", "components"])?` therefore returns `None`. That routes the
     file to `apply_loose_mat`'s `None` arm, which only recognises the file as present.
  2. **Properties** sit under each component's `Data` object, not on the component object itself.
  3. **Texture key**: `MRTextureFile` stores `"FileName" : "Data\\Textures\\QOG\\…\\TerminalCase_color.dds"`. The decoder reads
     `File`/`Path`, and it does not strip the `Data\` prefix.
  4. **Typed values are strings**: for example `"Enabled" : "false"` and `"MaterialOverallAlpha" : "0.750000"`. So `as_bool()`
     and `as_f64()` return `None`. A disabled `TextureReplacement` would be treated as **enabled**, because the code tests
     `enabled != Some(false)`, and it would push a fabricated flat colour.
  5. **Colour** is nested: `BSMaterial::Color → Data.Value → XMFLOAT4 {x,y,z,w}`, also as strings.

  `decode_rgba` also invents values when it cannot read one:
  - a missing channel becomes 1.0;
  - for a 6-digit hex value, alpha is read from the blue byte (`channel(6.min(len-2))` evaluates to `channel(4)`);
  - `Index as u8` wraps around.

  The loose arm also returns before `lookup_cdb_material`. So a loose file that cannot be decoded now hides a CDB row that
  previously resolved. That is latent: no installed loose file shadows a vanilla path (`portablegreenhouse02.mat` has no vanilla
  counterpart). Finally, the arm sits inside `starfield_cdb_gate` (`merge.rs:434`, `has_starfield_cdb()`), even though decoding a
  loose file does not need a CDB.
- **Evidence**:
  - `ba2_grep` over all 129 `Starfield/Data/*.ba2` found `.mat` entries in four archives: `qog-pawnshop - main.ba2` (12),
    `sp2_factionrequisitionkiosks - main.ba2` (5), `starfieldresourcerevival - main.ba2` (2) and `avontechshipyards - main.ba2`
    (1). That is 20 files, which matches `merge.rs:437-438` ("20 JSON `.mat` exports measured across 129 installed archives").
  - Two were extracted and read, `galacticpawnshopterminal_terminalcase.mat` and `lasersight_white.mat` (saved in
    `/tmp/audit/nifal/matsamples/`). Both have the `Objects`/`Components`/`Data`/`Type` layout and the string-typed values
    described above.
  - The doc claims "a re-scan of this install's 129 archives with `ba2_grep` finds **zero**" (`loose_mat.rs:10-11`) and
    "0 across this install's 129 archives" (`merge.rs:483`). Both are contradicted by that measurement.
  - The test `decodes_the_cited_component_spellings` builds its fixture in the invented layout, so it passes.
- **Impact**:
  - Every installed loose `.mat` contributes no textures or settings. These Creation surfaces render exactly as they did before
    #4277 (recognised as present only), while #4277 is closed and the code claims a working Stage A.
  - If the root lookup alone were fixed, the string-bool handling would *enable* authored-disabled replacements.
  - Once a loose override of a vanilla path exists, it would lose the CDB textures it gets today.
- **Related**: #4277 (closed), #762, #3398 (open, CDB per-field data), #5210 (open, CDB Phase-2 doc rot), #5284 (open, CDB tail
  copied at four sites), D8-02.
- **Suggested Fix**:
  - Re-derive the decoder from the installed samples: walk `Objects[].Components[]`, read `Data`, match `FileName`, strip the
    `Data\` prefix, parse string scalars and bools explicitly, and decode the `BSMaterial::Color` nesting.
  - Change the fixtures to real extracted files (or verbatim excerpts) and add a test that decodes one real-layout file end to
    end.
  - If a file does not decode, fall through to `lookup_cdb_material` rather than returning `PresenceOnly`.
  - Correct the "zero" census claims and the stale comments at `merge.rs:439-440` and `:573-575`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other three CDB-fill sites in `merge.rs`; module/merge doc census claims)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the material merge path, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

# FO3-2026-09-05-D2-01: the `NiSkinPartition` strip path bounds a *derived* triangle buffer against remaining stream bytes — 296 FO3 blocks lost, including every humanoid hand mesh

Labels: bug, nif-parser, critical, legacy-compat, nif, game:fo3

---

**Source**: `docs/audits/AUDIT_FO3_2026-09-05.md` (FO3-2026-09-05-D2-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

> **Severity escalated to CRITICAL at publish time.** The report filed this as HIGH from the FO3 measurement alone. The concurrent Oblivion audit then measured the same root cause costing 730/9,612 vanilla meshes and 4,693 blocks (8.1x amplification, no `block_sizes` table) including 24% of the game's `bhkLimitedHingeConstraint` ragdoll population, and explicitly recommended escalation. Three shipped titles lose content; two (FO4, Starfield) were measured immune.

- **Severity**: HIGH
- **Dimension**: 2 — NIF parser (defect is in the shared FO3/FNV/Oblivion parser)
- **Location**: `crates/nif/src/blocks/skin.rs` — `NiSkinPartition::parse`, the
  `num_strips > 0` arm. Introduced by `d49cd88b` ("Fix #3691: reserve skinning
  parse buffers", 2026-09-03).
- **Status**: Existing (report-only): `AUDIT_NIF_2026-09-04.md` `PERF-D6-NEW-01`.
  **No GitHub issue exists** — see the reconciliation note above. This entry adds
  the causal proof and the FO3 measurement that report deferred.
- **Description**: The strip branch pre-sizes its de-strip output with
  `stream.allocate_vec_sized::<[u16; 3]>(num_triangles as u32)?`. That delegates to
  `allocate_vec_min_bytes(count, size_of::<[u16;3]>()) = (count, 6)`, whose
  `check_alloc` **rejects the parse when `count * 6` exceeds the bytes remaining in
  the stream**. But `triangles` is not read from the stream at all — it is generated
  by `crate::blocks::strip::destrip` from the u16 strip arrays, which cost ~2 bytes
  per emitted triangle (a strip of `L` indices occupies `2L` bytes and yields `L-2`
  triangles). The bound over-demands by ~3×, so a perfectly valid partition whose
  strip payload sits near the end of a file is rejected with `UnexpectedEof`.
  `allocate_vec_min_bytes`'s own doc states the rule this violates verbatim: *"the
  caller-supplied minimum must never exceed the actual smallest legitimate on-disk
  encoding for one element, or this rejects valid files."*
- **Evidence** (all measured this run at HEAD `da5cecb7`):
  - `cargo test --release -p byroredux-nif --test parse_real_nifs parse_rate_fallout_3 -- --ignored`:

    ```
    [Fallout 3/Fallout - Meshes.bsa] 10989 NIFs, 10726 clean, 263 truncated, 0 failed
    [Fallout 3/Anchorage - Main.bsa]  1597 NIFs,  1592 clean,   5 truncated, 0 failed
    [Fallout 3/BrokenSteel - Main.bsa] 855 NIFs,   853 clean,   2 truncated, 0 failed
    [Fallout 3/PointLookout - Main.bsa] 1372 NIFs, 1360 clean, 12 truncated, 0 failed
    [Fallout 3/ThePitt - Main.bsa]    1614 NIFs,  1614 clean,   0 truncated, 0 failed
    [Fallout 3/Zeta - Main.bsa]        745 NIFs,   733 clean,  12 truncated, 0 failed
    [Fallout 3] parsed 17172/17172: clean 98.29% (16878 / 294 truncated / 0 failed)
    ```

  - `per_block_baseline_fallout_3` — **FAILS**:
    `UNKNOWN grew NiSkinPartition 0 -> 296` · `PARSED shrank NiSkinPartition 3099 -> 2803`.
  - `unknown_ceiling_fallout_3` — **FAILS**:
    `NiUnknown recovery count grew 0 -> 296 (526109 blocks total)`.
  - `trace_block` on `meshes\characters\_male\lefthand.nif`:
    `[ 8] @ 63182 NiSkinPartition size=21272 ... ERR at consumed 14093: NIF requested 12162-byte read at position 77275, only 8615 bytes remaining in 85890-byte stream`
    (12 162 / 6 = 2 027 triangles).
  - **Causal proof**: replacing the one call with
    `stream.allocate_vec_min_bytes::<[u16; 3]>(num_triangles as u32, 2)?` and
    re-running the gate yields
    `[Fallout 3] parsed 17172/17172: clean 100.00% (17172 clean / 0 truncated / 0 failed)`.
    Patch reverted; `git status` shows `crates/nif/src/blocks/skin.rs` unmodified.
- **Impact**:
  - **Vanilla FO3 humanoids.** `meshes\characters\_male\lefthand.nif` and
    `righthand.nif` both fail. These are exactly the two meshes `humanoid_body_paths`
    loads alongside `upperbody.nif` for every kf-era FO3 NPC (#793 / M41-HANDS) —
    i.e. every Megaton dweller. (`upperbody.nif`, `femaleupperbody.nif` and
    `headhuman.nif` are clean.)
  - **Creatures and armour.** Truncated examples span
    `creatures\smbehemoth\smbehemothmedium.nif`, `creatures\brahmin\brahminwater.nif`,
    `creatures\radscorpion\albino.nif`, `creatures\mirelurkking\dlc04swamplurk.nif`,
    `dlc05\creatures\alien\dlc05alien2.nif`, `dlc05\creatures\maintenancerobot\mb01.nif`,
    plus armour/headgear (`raiderarmor04\hatf.nif`,
    `dlcanch\armor\chinesestealtharmor\m\glover.nif`, `snowcombatarmor\m\helmet.nif`).
  - **Downstream, silently.** When the partition becomes `NiUnknown`,
    `scene.get_as::<NiSkinPartition>` returns `None` and
    `import/mesh/skin.rs::triangle_body_parts` returns `Vec::new()`. That function's
    own comment names the consequence: *"at which point `hide_skin_partitions` stops
    hiding anything and every NPC renders bare body skin through their armour, with
    `cargo test` and the parse-rate gate both still green."*
  - **Blast radius beyond FO3 (shared parser, not FO3-only).** FNV measured this run:
    `Fallout - Meshes.bsa` 14 881 NIFs → 14 347 clean / **534 truncated**, plus
    DeadMoney 35, HonestHearts 34, OldWorldBlues 77, LonesomeRoad 51,
    GunRunnersArsenal 4, CaravanPack 2, ClassicPack 2, MercenaryPack 2 — **≥ 741**.
    Oblivion is the `PERF-D6-NEW-02` 92.41 % figure. LE/converted Skyrim content
    authors strips too.
- **Related**: `#3691` (the reservation request whose fix introduced this),
  `#2523` (the `allocate_vec_sized` / `allocate_vec_min_bytes` split being violated),
  `#1549` (de-strip), `#3875` (report-only findings with no issue trace),
  `AUDIT_NIF_2026-09-04.md` `PERF-D6-NEW-01` / `PERF-D6-NEW-02`.
- **Suggested Fix**: `stream.allocate_vec_min_bytes::<[u16; 3]>(num_triangles as u32, 2)?`.
  Two bytes is the honest per-triangle floor for a strip encoding. Add a regression
  fixture with a strip of ≥ 5 indices positioned so `3L` exceeds the remaining bytes —
  the commit's existing test (one strip of length 4) clears the current bound by one
  byte and therefore cannot fail.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary


# TD2-2026-10-15-18: 22 `crates/plugin/examples` each hand-roll an unbounded recursive GRUP walk over `EsmReader`, four with the same `OwnedSub` callback shape

Labels: low,esm-plugin,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-18`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/plugin/examples/{perk_fragment_census,dump_qust_vmad_fragments,probe_objective_fragments,qust_alias_script_census}.rs` (identical `type OwnedSub` + `walk(reader, end, want, f)`), and `autocalc_census`, `ctda_function_census`, `ctda_variable_census`, `corpse_marker_census`, `dump_npc_subs`, `dump_pack_subs`, `dump_record_subs`, `dump_scen_subs`, `dump_voli_subs`, `dump_wthr_subs`, `dump_wths_refl`, `esm_dim8_coverage`, `health_gmst_dump`, `probe_npc_perks`, `probe_weap_vats`, `qust_alias_rawdump`, `water_current_census`, `watr_wind_census`; production home `crates/plugin/src/esm/records/grup_walker.rs:96` (`extract_records`, `pub(super)`)
- **Status**: NEW (67 examples; 15 added 2026-10-12..14)
- **Age**: copies since 2026-08; 10 new on 10-12..14
- **Effort**: medium
- **Description**: `while reader.position() < end && reader.remaining() > 0 { if reader.is_group() { read_group_header; group_content_end; recurse } else { read_record_header; match type { want => read_sub_records, _ => skip_record } } }`, 22 files with `group_content_end`, 23 with `read_group_header`. The production walker is the bounded, depth-checked version (`bounded_group_content_end`) but is not reachable from examples; the example copies are unbounded recursion, so a corrupt real ESM can stack-overflow a census run.
- **Evidence**: `grep -l group_content_end crates/plugin/examples/*.rs | wc -l` = 22.
- **Impact**: each new ESM probe (the M47.2 census cadence is 3-5 a day) clones the walker again; none gets the production depth bound.
- **Related**: `/audit-esm`, #5485 (the NIF-side revisit guard).
- **Suggested Fix**: expose `EsmReader::for_each_record(&mut self, want: Option<&[u8;4]>, f: impl FnMut(&RecordHeader, Vec<SubRecord>))` (depth-bounded, built on `bounded_group_content_end`) in `reader.rs`, and migrate the examples; `dump_*_subs.rs` (6 files, 745 lines) collapse into `dump_record_subs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)

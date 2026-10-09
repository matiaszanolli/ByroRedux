# #5393: GAME-D2-2026-10-08-02: Dialogue voice maps a FormID's load-order slot to the wrong plugin in every multi-plugin session (slot 0 = `--esm`, but masters own slot 0) — and re-implements an existing correct helper

**Labels**: medium,gameplay,dialogue,audio,bug,game:fnv,game:fo3,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5393

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D2-2026-10-08-02` (HEAD `00f580e09`)

- **Severity**: MEDIUM
- **Dimension**: Dim 2
- **Location**: `byroredux/src/systems/dialogue_voice.rs:66-85`; correct helper at `byroredux/src/cell_loader/load_order.rs:315`; slot assignment at `load_order.rs:591-` and `cell_loader/load.rs:426-430`
- **Status**: NEW
- **Trigger**: FO3/FNV with any `--master` (e.g. `--master FalloutNV.esm --esm DeadMoney.esm`, the documented DLC route), any voiced line.
- **Description**: `plugin_file_for` treats byte 0 as `LoadedPluginSet.esm_path` and byte *k* as `masters[k-1]`. The loader builds the load order as `masters…, esm`, and `#1554` global-slot assignment hands regular slots out in that order. So masters occupy 0..n-1 and the `--esm` takes slot n.
  - A vanilla FalloutNV INFO (slot 0) composes `sound\voice\deadmoney.esm\…`.
  - A DLC INFO composes `falloutnv.esm\…`.
  - ESL slots (0xFE) and medium slots (0xFD) are not handled.

  Every line misses and falls back to the subtitle estimate without any warning, by design. `load_order::plugin_for_form_id` already resolves slot → basename correctly, including ESL sub-indices, and has three tests. The single-plugin smoke (`dt1`) cannot see this, because there slot 0 really is the `--esm`.
- **Evidence**: `let raw = if byte == 0 { esm_path.as_str() } else { masters.get(byte - 1)… }` versus `plugin_paths = masters.iter().chain(once(esm_path))`.
- **Impact**: Phase V voice is silent for every line in any DLC or multi-master FO3/FNV session.
- **Related**: #5367 (V).
- **Suggested Fix**: Resolve through the session's `LoadOrder` with `plugin_for_form_id` (widen its visibility and keep the `LoadOrder` reachable as a resource) instead of the duplicate mapping.

## Also reported as `LC-D3-02` (AUDIT_LEGACY_COMPAT_2026-10-08.md)

Cross-report duplicate merged at publish time; the sibling report's text follows.

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-08.md` — `LC-D3-02` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `plugin_file_for` (`byroredux/src/systems/dialogue_voice.rs:66-84`) still maps byte 0 → `esm_path` and byte k → `masters[k-1]`.

- **Severity**: MEDIUM. Silent loss of the authored input across every multi-plugin session, which is the normal shape once any DLC is loaded.
- **Dimension**: 3 — Cross-game translation (load-order identity re-derived by hand)
- **Location**: `byroredux/src/systems/dialogue_voice.rs:66-85`
- **Status**: NEW. `f8950e7cc`. The defect class is the one `#3366` fixed in `cell_loader/load_order.rs::plugin_for_form_id`.
- **Description**:
  - Load order is masters first, then the main plugin: `cell_loader/load.rs:389-392` chains `masters` and then `esm_path` into `parse_record_indexes_in_load_order`, so slot 0 is the first `--master`.
  - `plugin_file_for` maps `byte == 0` to `esm_path` and `byte k` to `masters[k-1]`. That is correct only when there are no masters.
  - It also reads the top byte as a load-order position. `plugin_for_form_id`'s `#3366` doc explains why that breaks once a light master is present (slot ≠ position, and the `0xFE` space). The engine already installs `GlobalFormIdResolver` (`load_order.rs:268`, `resolve(form_id) -> FormIdPair`) for exactly this lookup.
- **Evidence**:
  ```rust
  // dialogue_voice.rs:73-78
  let byte = (form_id >> 24) as usize;
  let raw = if byte == 0 {
      esm_path.as_str()
  } else {
      masters.get(byte - 1).map(String::as_str)?
  };
  ```
  With `--master FalloutNV.esm --esm DeadMoney.esm`, Doc Mitchell's `0x00107222` resolves to `sound\voice\deadmoney.esm\…` and Dead Money's `0x01…` lines resolve to `falloutnv.esm`. Neither exists. The `dt1-dialogue-layers.sh` gate passes only because it runs `--esm FalloutNV.esm` with no master.
- **Impact**: Every voice line fails in a session with masters (DLC interiors, or FO3/FNV plus any DLC). Combined with LC-D3-01, this is total voice loss there, and it falls back silently to the subtitle estimate.
- **Related**: LC-D3-01, `#3366` (closed), `#5367`.
- **Suggested Fix**: Resolve the owning plugin through `GlobalFormIdResolver::resolve(info.form_id)`, which gives the plugin name and the local id that LC-D3-01 also needs. Delete `plugin_file_for`. Add a `--master` + `--esm` unit test.

## Completeness Checks
- [ ] **SIBLING**: other FormID-byte → plugin-name mappings replaced by load_order::plugin_for_form_id
- [ ] **TESTS**: A regression test pins this specific fix

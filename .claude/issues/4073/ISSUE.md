# #4073 — ESM-2026-09-09-D6-02

a localized plugin that resolves **zero** string tables produces no diagnostic at any log level

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4073 --json state`).

---

- **Severity**: MEDIUM
- **Dimension**: Localized Strings
- **Record / Sub-record**: —
- **Location**: `crates/plugin/src/esm/strings_table.rs:242-262`; `byroredux/src/cell_loader/load_order.rs:400-417`
- **Status**: NEW
- **Description**: `load_file` logs a `debug!` on success and a `warn!` only when
  a file was *found but failed to parse*. A file that is simply **absent** —
  loose miss followed by archive miss — returns `None` silently. `StringTableSet`
  then has all three fields `None`, `install_strings_guard` installs that empty
  set unconditionally, and `read_lstring_or_zstring` quietly hands back a
  placeholder for every field. The engine never says "this plugin is localized
  and I found no tables", which is precisely why D6-01 above can be 100 % broken
  on three games and still look like normal operation.
- **Evidence**:
  - `strings_table.rs:245-261` — the absent-file arm carries no logging:
    ```rust
    let (data, source) = match std::fs::read(&path) {
        Ok(data) => (data, path.display().to_string()),
        Err(_) => {
            let archive_path = format!(r"strings\{name}");
            (read_archive(&archive_path)?, archive_path)   // <- silent give-up
        }
    };
    match StringsTable::parse(&data, has_prefix) {
        Ok(t) => { log::debug!("loaded {} ({} entries)", source, t.len()); Some(t) }
        Err(e) => { log::warn!("failed to parse {}: {e}", source); None }
    }
    ```
  - `load_order.rs:400-417` — the guard is installed whether or not anything loaded:
    ```rust
    let tables = esm::StringTableSet::load_with_archive(plugin_path, language, |relative_path| {
        read_archive(plugin_path, relative_path)
    });
    Some(esm::StringsTableGuard::new(tables))
    ```
  - `records/common.rs:193-202` — the miss path is placeholder-only, no counter,
    no warn (this part is *correct* per checklist item 4; the gap is that nothing
    ever aggregates it).
- **Impact**: A whole-game localization failure is indistinguishable from normal
  operation in the logs. Diagnosis currently requires noticing `<lstring 0x…>` in
  rendered UI or in `list_cells` output (which deliberately *hides* it —
  `byroredux/src/list_cells.rs:148-151`).
- **Related**: ESM-2026-09-09-D6-01 (this is what makes it silent).
- **Suggested Fix**: In `install_strings_guard`, after `load_with_archive`, emit a
  single `log::warn!` per plugin when `strings`, `dlstrings` and `ilstrings` are
  all `None` — naming the plugin, the language token tried, and the composed
  path. One line, once per plugin, no per-form spam.

---

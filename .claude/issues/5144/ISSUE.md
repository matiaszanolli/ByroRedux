# null: TOOL-D4-2026-09-29-02: `settings-io` silently discards the stored map when the existing file fails to parse, so the next save erases input bindings

labels: bug, medium, tech-debt
state: OPEN

**Source report**: `docs/audits/AUDIT_TOOLING_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: Boot Handoff & Persistence (exposure: players and launcher users; also triggerable by an extension's settings write with no user action)

## Location
- `crates/settings-io/src/lib.rs` `save_to_path` (`.unwrap_or_default()`, `Err(_) => BTreeMap::new()`) and its contract doc
- Save triggers: `tools/byro-launcher/src/settings_screen.rs`, `byroredux/src/extensions/systems.rs`, `byroredux/src/main.rs`

## Description
- `save_to_path` is documented as "**preserving stored keys it does not know** … required by the launcher", because the launcher's registry has no input bindings.
- That contract only holds when the existing file parses. For a hand-edit typo, a torn file (TOOL-D4-01) or a future `SETTINGS_VERSION` with a changed shape, `toml::from_str` fails; `unwrap_or_default()` substitutes an empty map, and the file is rewritten with only the saving registry's keys.
- Nothing is logged at save time. `load` warned once at startup, but that says the file was skipped, not that it is about to be erased.
- A parseable newer-version file is also silently re-stamped `version = 1`: `load` logs, `save` does not.

## Evidence
```rust
Ok(existing) => toml::from_str::<StoredSettings>(&existing)
    .map(|stored| stored.settings)
    .unwrap_or_default(),
Err(_) => BTreeMap::new(),
```

## Impact
Opening the launcher's settings screen and pressing Save with a malformed `settings.toml` destroys every key rebinding irreversibly. An extension writing a setting does the same inside the engine, with no user action.

## Related
TOOL-D4-2026-09-29-01, #3472, #4974. Label gap: settings-io / launcher have no own label → `tech-debt`.

## Suggested Fix
When the existing file does not parse, refuse to save with a warning, or first copy it aside (`settings.toml.bad`) and log that. Treat a read error other than NotFound the same way.

Validated at HEAD 9fcfdc3fc: `save_to_path` still maps a parse failure to `unwrap_or_default()` and any read error to `BTreeMap::new()`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (overrides.rs `merge_into_file` / boot-request merge semantics)
- [ ] **TESTS**: A regression test pins this specific fix


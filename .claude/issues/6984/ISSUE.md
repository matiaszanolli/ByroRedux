# #6984: TD7-2026-11-15-05: SDK / mod-runtime policy ceilings are typed twice (`MAX_DISPLAY_NAME_BYTES`, and the 16 KiB / 64 KiB string and blob caps)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6984
- **Labels**: low,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 7 (magic numbers)
- **Location**: `crates/mod-runtime/src/identity.rs:5` and `crates/sdk/src/manifest.rs:26` (both private `const MAX_DISPLAY_NAME_BYTES: usize = 256`); `crates/sdk/src/storage.rs:228-229` and `crates/sdk/src/component.rs:179-180` (`max_string_bytes: 16 * 1024`, `max_blob_bytes: 64 * 1024` in two `Default` impls); prose copy in `crates/mod-runtime/src/limits.rs:37-38`
- **Age**: not dated (never-swept crates)
- **Effort**: trivial
- **Labels**: `tech-debt`
- **Status**: NEW
- **Description**: `mod-runtime` already depends on `byroredux-sdk` (`mod-runtime/Cargo.toml:8`) and imports from it in the same file as the display-name copy (`identity.rs:1`), so the SDK constant could be imported. The two stores (`PrincipalStorageLimits`, `ComponentStoreLimits`) take the same 16 KiB / 64 KiB defaults; `limits.rs:36-38` documents "storage strings 16 KiB and blobs 64 KiB by default" as the reason a 2 MiB hostcall lift budget "dwarfs" them, which makes the numbers a sized relationship stated only in a comment.
- **Impact**: low; defaults only (`ExtensionHost::new(SandboxConfig::default(), ComponentStoreLimits::default())` is the only production construction, `byroredux/src/extensions/systems.rs:20`). A policy change made in one place leaves the other store or the principal-name check on the old value. Note for whoever raises these limits: `SandboxConfig::hostcall_fuel` (`limits.rs:157-161`) considers only `MAX_HOSTCALL_LIFT_BYTES` and the log-message cap, not the configured store caps, so a store cap above about 2 MiB would trap guest writes. Unreachable today.
- **Related**: `crates/mod-runtime/src/limits.rs:33-44`.
- **Suggested Fix**: make the SDK constant `pub` and import it in `identity.rs`; define `DEFAULT_MAX_STRING_BYTES` / `DEFAULT_MAX_BLOB_BYTES` once in the SDK and use them in both `Default` impls; add a `const _: () = assert!(DEFAULT_MAX_BLOB_BYTES <= MAX_HOSTCALL_LIFT_BYTES)` style check on the mod-runtime side.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix

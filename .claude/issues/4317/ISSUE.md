# #4317 SCR-D1-2026-09-14-01: every `.pex` string-table reference is cloned into an owned `String`, so a ~330 KB wire-valid file exhausts 8 GB and aborts the process uncatchably

**Labels**: high,scripting,safety,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: HIGH (domain table: unbounded alloc reachable from untrusted `.pex`; same class as #3783)
- **Dimension**: PEX Reader & Opcode Decode
- **Untrusted-Input**: Yes
- **Location**: `crates/pex/src/reader.rs` `Reader::string_index` (`self.strings.get(idx).cloned()`), called by every table reader (`read_user_flags`, `read_typed_names`, `read_debug_info`, `read_properties`, `read_function`, `value()` tags 1/2); `crates/pex/src/model.rs` (owned `String` fields); `crates/pex/src/call_sites.rs` `scan_function` (per-call `CallSite` clones of `source_file_name` / `object.name` / scope)
- **Status**: NEW. Searched "string_index", "pex string clone", "pex OOM", "pex allocation amplification". Nearest are #3783 (stack overflow), #1710 (var-arg pre-alloc) and #55 (NIF string table); none cover this. The 08-20 and 09-06 reports checked only `with_capacity` counts.
- **Description**: The string table allows one string of up to 65,535 bytes. A `u16` index costs 2 bytes on disk but allocates a fresh full copy per reference. Containers repeat (objects × states × functions × params × instructions), so total allocation is bounded only by file size, at up to ~32,000× per reference. Champollion's `getStringIndex` (`FileReader.cpp:504-518`) returns an index handle and never copies. Rust aborts on allocation failure instead of panicking, so `translate::catching_panics` (the #3948 net around parse → preflight → `call_sites` → decompile) cannot recover, and the engine dies during cell load. Reachable via `ScriptProvider::resolve_pex`, whose doc tells users to list mod archives in `--scripts-bsa`.
- **Evidence** (orchestrator confirmed the clone at `reader.rs:139-147` and the absence of any size budget in `reader.rs`/`lib.rs`; measurements from Dim 1's probe `d1fuzz/src/bin/amp.rs`, wire-valid FO4 LE `.pex` with one 65,535-byte string):

  | Shape | File | Peak RSS | Ratio |
  |---|---|---|---|
  | 10,000 user flags | 95.6 KB | 628 MB after `parse` | 6,861× |
  | 10,000 params (2 refs each) | 105.6 KB | 1,253 MB after `parse` | 12,418× |
  | 65,535 params, one function | ≈327 KB | — | **aborts: `memory allocation of 65535 bytes failed`** (`ulimit -v 8000000`) |
  | 20,000 `callstatic` + `call_sites()` | 365.6 KB | 1,257 MB after `parse`, **3,762 MB after `call_sites`** | 3,597× (parse) |

- **Impact**: One malicious or corrupt mod `.pex` of a few hundred KB crashes the engine at cell load, with no fallback. It is the memory-side twin of the CPU DoS fixed in #3938, which only stalled for 11.6 s; here the process dies. Vanilla content is unaffected (`actor.pex` is 13 KB).
- **Related**: #3938, #3783, #1710, #3948
- **Suggested Fix**: Mirror Champollion. Intern the table as `Arc<str>` in `read_string_table` and have `string_index` return `Arc::clone`, or store `u16` indices; apply the same to `CallSite`. Stopgap: a pre-parse budget (e.g. Σ referenced-string bytes ≤ 64 × `bytes.len()`) behind a new `PexError` variant. Regression test: the 65,535-params shape parses in bounded memory or returns `Err`, and never aborts.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix

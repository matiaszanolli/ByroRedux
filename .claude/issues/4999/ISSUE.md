# #4999: CONC-D7-2026-09-28-01: On Windows, "lock-free positional reads" still serialise in the kernel for each archive handle

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,import-pipeline,concurrency,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW (doc accuracy + perf on a secondary platform; no correctness defect)
- **Dimension**: Worker Threads
- **Location**: `crates/bsa/src/read_at.rs:1-7` (module claim), `:24-48` (Windows impl); the claim is repeated at `byroredux/src/streaming.rs:669-672`, `:1557-1558` and `:1866-1869`, and in `docs/engine/archives.md:360-361,466`
- **Status**: NEW
- **Description**: The module doc says positional reads let "concurrent extracts from one archive need no lock and never wait on each other". That holds on Unix (`pread`). On Windows, `seek_read` calls `ReadFile` with an `OVERLAPPED` offset. `File::open` returns a *synchronous* handle (no `FILE_FLAG_OVERLAPPED`), and the NT I/O manager serialises every I/O request on a synchronous file object through its file-object lock. So:
  - **Correctness holds.** Each call carries its own offset, the cursor side effect is inert (no cursor read happens after `open`; see `archive/open.rs:445-452`, `ba2.rs:367`, `csg.rs:152-189`), and the short-read loop handles partial reads.
  - **The reads do wait on each other.** Only the syscall is serialised. Inflate still runs in parallel, so #3659's goal survives.
- **Evidence**: `read_at.rs:27-28` (the comment covers the cursor side effect but not the serialisation). I cross-compiled with `cargo check -p byroredux-bsa --target x86_64-pc-windows-gnu`: it compiles cleanly. There is no Windows CI (`.github/workflows/*` are all self-hosted Linux), so the Windows loop has never run.
- **Trigger Conditions**: A Windows build streaming exterior cells, where the stream-pool tasks and main-thread texture resolves hit one archive handle.
- **Impact**: Per-archive read throughput on Windows is serial. The docs overstate the win, which could mislead a future perf investigation.
- **Verification Path**: Profile a Windows build with more than 8 fresh NIFs per cell, or ask for one handle per thread and compare.
- **Related**: #3659, #360, #1170
- **Suggested Fix**: Qualify the doc ("need no user-space lock; Windows synchronous handles still serialise the syscall"). If Windows throughput ever matters, open the handle with `FILE_FLAG_OVERLAPPED` through `OpenOptionsExt::custom_flags` and wait per call.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D7-2026-09-28-01) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

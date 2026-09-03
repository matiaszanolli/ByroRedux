# #3449 — SAFE-2026-08-27b-05: debug-server is a default cargo feature and its accept loop spawns an uncapped OS thread per connection

**Severity**: LOW (hardening) · **Location**: `crates/debug-server/src/listener.rs::listener_loop`

## Fix

Verified the premise: `listener_loop`'s accept-`Ok` arm spawned a named
OS thread (`thread::Builder::new().spawn(...)`) for every accepted
connection unconditionally — no cap, no rate limit, no authentication.
`debug-server` is in `byroredux/Cargo.toml`'s `default` feature set, so
an ordinary `cargo build --release` produces a binary that listens on
loopback and can be exhausted by a local process opening connections in
a tight loop.

Added `pub(crate) const MAX_CONCURRENT_CLIENTS: usize = 8`, documented
next to the existing `MAX_QUEUED_COMMANDS` constant with the same
"loopback-only, operator-controlled attack surface" framing (#857).
Folded the cap check into the *existing* critical section that already
prunes `active_streams`' dead `Weak<TcpStream>` entries before pushing a
new one: after the prune, `active.len()` is already the live-connection
count, so the cap reuses it directly rather than the issue's own
suggested separate `AtomicUsize`. A connection past the cap has its
`Arc<TcpStream>` dropped (closing the socket) and the outer loop
`continue`s — no thread is ever spawned for it.

This is a deliberate deviation from the issue's own "Suggested Fix"
(a separate incremented/decremented counter): reusing `active_streams`'
own post-prune length avoids a second lock or a second counter that
could desync from the real connection count, which better satisfies the
issue's own LOCK_ORDER checklist item than the suggestion does. Matches
this session's "prioritize improving existing code over duplicating
logic" convention, and mirrors this same file's existing
`MAX_QUEUED_COMMANDS` / `try_enqueue_command` precedent.

## SIBLING (issue's own checklist item — "any other accept/spawn loop —
`tools/byro-dbg`'s client side, the screenshot channel")

- `tools/byro-dbg/src/main.rs`: a single outbound `TcpStream::connect` —
  the CLI's own one connection, not a server-side accept loop. No cap
  applicable.
- `tools/byro-dbg/src/tui.rs::spawn_net_thread`: spawns exactly one
  long-lived worker thread per TUI session, owning the single connection
  the CLI itself opened — not a per-incoming-connection accept loop.
  No cap applicable.
- The screenshot channel (`crates/debug-server/src/system.rs`) has no
  `TcpListener`/`thread::Builder`/`.spawn()` of its own — it consumes
  the same command queue `listener_loop` already feeds, no separate
  accept path exists.

No other unbounded accept/spawn loop found in the workspace.

The issue's separate suggestion — "decide deliberately whether
`debug-server` should remain in `default` … and record that decision
next to the feature" — is a product decision, not a code defect; leaving
it as `default` is the existing, working decision (loopback-only bind is
the stated mitigation for exactly this), so no change made. Recording it
here rather than in `Cargo.toml` since there is nothing to a comment
that isn't already said by this fix and #857.

## LOCK_ORDER (issue's own checklist item — "the counter must not widen
the existing `active_streams` mutex critical section or introduce a
second lock inside it")

Satisfied by construction: the cap check is a single `if` added inside
the *same* `active_streams.lock()` critical section that already does
the prune-then-push, adding zero new locks and not widening the
section's own logical scope (still one prune + one conditional push,
just with the push now conditional).

## TESTS (issue's own checklist item — "a regression test pins this
specific fix")

`connections_past_the_cap_are_refused` — spins up a real listener via
`spawn(0)`, opens `MAX_CONCURRENT_CLIENTS` real `TcpStream::connect`
connections (kept alive so the opportunistic prune can't free a slot
mid-test), then opens one more and asserts it observes a clean EOF
(`read()` returns `Ok(0)`) within a bounded timeout — proving the
listener closes the extra connection immediately rather than accepting
it and spawning a thread.

**Reintroduce-and-revert verification**: temporarily restored the
pre-fix unconditional push (`active.retain(...); active.push(...); false`)
— confirmed the new test failed (`WouldBlock` on the refused
connection's read, since with no cap the connection is accepted and
never closed, so no EOF ever arrives within the 2 s read timeout).
Restored the fix and reran — all 15 tests in `debug-server` pass again.

## Verification

- `cargo check -p byroredux-debug-server --tests`: clean, zero warnings.
- `cargo test -q -p byroredux-debug-server`: 15 passing, 0 failing (+1
  new).
- `cargo test -q --no-fail-fast` (full workspace): see commit for count.

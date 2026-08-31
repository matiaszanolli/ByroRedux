# byro-detect

Finds installed Bethesda titles, checks whether the engine can actually load
them, and optionally remembers where they are.

This is the launcher's install-discovery path with no window, no GPU, and no
engine — which makes it both the way to develop that path and the fallback for
someone whose machine cannot start the launcher at all.

```bash
cargo run -p byro-detect                       # report what is installed
cargo run -p byro-detect -- --write            # also remember the paths
cargo run -p byro-detect -- --profiles <path>  # use a different profiles file
```

## Why `--write` matters

The shipped profile registry defers each game's absolute path to a
`--games-root` whose default is one developer's Steam library. `--write` records
each detected data directory in the `[roots]` table of
`~/.byroredux/profiles.toml`:

```toml
[roots]
fnv = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data"
```

The engine's profile loader applies that over the shipped registry, touching
only `root` — so `cargo run -- --game fnv --cell GSProspectorSaloonInterior`
resolves correctly on a machine that is not the dev box, without a
`--games-root` flag or an environment variable.

A `[roots]` entry deliberately cannot carry archive lists. If it could, a
detection run would freeze a copy of today's lists and shadow any later engine
update that adds an archive. Curated `[profiles.*]` blocks and `[defaults]` in
the same file survive a write untouched.

## Output

```
Fallout: New Vegas — ready  [Steam]
  /mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data
  ok    Main plugin: FalloutNV.esm (234 MB)
  ok    Meshes: 1 archive(s) will load
  ok    Textures: 2 archive(s) will load
```

Archive counts include auto-loaded numbered siblings (FNV's
`Fallout - Textures2.bsa`, Skyrim's `Textures1..8`), using the same rule the
engine's asset provider applies — a sibling that does not exist is never
reported as missing.

`warn` does not gate launching; `FAIL` does. A present-but-unopenable archive is
always a `FAIL`, even in an otherwise optional category, because the engine
would fail mid-load rather than start degraded.

## Scope

Steam only. The GOG and Windows-registry probes described in
[`docs/engine/launcher.md`](../../docs/engine/launcher.md) §3.1 are not
implemented; a non-Steam install is reachable by hand-writing a `[roots]` entry,
which this tool prints instructions for when it finds nothing.

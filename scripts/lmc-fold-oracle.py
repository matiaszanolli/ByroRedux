#!/usr/bin/env python3
"""Fold conformance oracle (PRD legacy-mod-compatibility LMC-REC-002, -004, -007, -011).

Reads every plugin of a load order raw, with no engine code, and applies the game's rule: each record is decided by
the last plugin in load order that carries it, a deleted (0x20) winner removes it, and a placed reference sits in the
cell its winning copy is authored under. It then runs the engine's fold dump
(`byroredux/src/cell_loader/fold_conformance_tests.rs`) and reports every form where the fold disagrees:

  - references missing, extra, in the wrong cell, or carrying a loser's base form or position;
  - navmeshes kept although their winner deletes them, missing, or under the wrong cell;
  - forms the fold lacks or holds with another record type, and forms it holds that no plugin carries;
  - the fold's provenance winner (LMC-REC-007) differing from the last carrier;
  - tombstoned forms still held by any record category.

A FormID whose mod index points past its plugin's master list cannot be resolved by a documented rule (#7329); the
engine passes it through, which aliases another plugin's slot. Those forms are reported apart, as known, and do not
fail the run. Informational: CELL-record fields an intermediate override changed that the winning override drops
(the game's whole-record rule; the `merge` policy does not cover CELL).

usage: lmc-fold-oracle.py --game fo3|fnv|sse|fo4 --plugins FILE --binary TEST_BINARY [--work DIR] [--examples N]
Exit 0 when the fold conforms, 1 when it does not, 2 on a usage or input error.
"""
import argparse, json, os, struct, subprocess, sys, tempfile, zlib
from collections import Counter, defaultdict

PLACED = {b'REFR', b'ACHR', b'ACRE', b'PGRE', b'PMIS', b'PHZD', b'PARW', b'PBAR', b'PBEA', b'PCON', b'PFLA'}
FORMID_TAGS = {b'XCLR', b'XCIM', b'XCWT', b'XEZN', b'XLCN', b'LTMP', b'XCMO', b'XCAS', b'XCCM', b'XOWN', b'XILL',
               b'XWEM', b'XCGD'}
# CELL sub-records left out of the lost-edit tally: precombine and visibility data, and XCLC, whose bytes after the
# grid and land flags are uninitialised padding that differs between every carrier.
SKIP_TAGS = {b'XCRI', b'XPRI', b'VISI', b'RVIS', b'PCMB', b'MHDT', b'XWCN', b'XWCS', b'XWCU', b'TVDT', b'XCLC'}
FLAG_DELETED, FLAG_PARTIAL, FLAG_COMPRESSED = 0x20, 0x4000, 0x40000
TEST = 'cell_loader::load_order::fold_conformance_tests::dump_fold'


def subrecords(data):
    i, big = 0, None
    while i + 6 <= len(data):
        tag = data[i:i + 4]
        size = struct.unpack_from('<H', data, i + 4)[0]
        i += 6
        if tag == b'XXXX':
            big = struct.unpack_from('<I', data, i)[0]
            i += size
            continue
        if big is not None:
            size, big = big, None
        yield tag, data[i:i + size]
        i += size


class Plugin:
    def __init__(self, path, game, position):
        self.path, self.position = path, position
        self.name = os.path.basename(path).lower()
        with open(path, 'rb') as f:
            self.buf = f.read()
        rtype, size, flags = struct.unpack_from('<4sII', self.buf, 0)
        if rtype != b'TES4':
            raise ValueError('%s: not a plugin' % path)
        self.masters = [d[:-1].decode('cp1252').lower() for t, d in subrecords(self.buf[24:24 + size]) if t == b'MAST']
        self.light = game in ('sse', 'fo4') and bool(flags & 0x200 or self.name.endswith('.esl'))
        self.header_end = 24 + size


class Resolver:
    """Plugin-local FormID -> (owner position, object id). An index past the master list is not resolvable (#7329):
    it keys as ('X', raw id), which cannot collide with a resolved form, and later maps to the key the engine's
    pass-through gives it."""

    def __init__(self, plugins):
        self.by_name = {p.name: p for p in plugins}

    def of(self, plugin):
        masters, by_name = plugin.masters, self.by_name

        def key(local):
            if local == 0:
                return None
            idx = local >> 24
            if idx > len(masters):
                return ('X', local)
            if idx < len(masters):
                owner = by_name.get(masters[idx])
                if owner is None:
                    return (-1, local)
            else:
                owner = plugin
            return (owner.position, local & (0xFFF if owner.light else 0xFFFFFF))
        return key


def walk(plugin, key, records, cells, placed, navm):
    buf = plugin.buf

    def data_of(off, size, flags):
        data = buf[off:off + size]
        return zlib.decompress(data[4:]) if flags & FLAG_COMPRESSED else data

    def walk_group(start, stop, cell):
        i = start
        while i < stop:
            tag = buf[i:i + 4]
            if tag == b'GRUP':
                gsize, label, gtype = struct.unpack_from('<IIi', buf, i + 4)
                walk_group(i + 24, i + gsize, key(label) if gtype in (6, 8, 9, 10) else cell)
                i += gsize
                continue
            size, flags, fid = struct.unpack_from('<III', buf, i + 4)
            body, k = i + 24, key(fid)
            i = body + size
            if k is None:
                continue
            deleted = bool(flags & FLAG_DELETED)
            records[k] = (tag, deleted)
            if tag == b'CELL':
                fields = {}
                if not deleted:
                    for t, d in subrecords(data_of(body, size, flags)):
                        if t in SKIP_TAGS:
                            continue
                        if t in FORMID_TAGS and len(d) % 4 == 0:
                            d = tuple(key(x) for x in struct.unpack_from('<%dI' % (len(d) // 4), d))
                        fields[t] = d
                cells[k].append((plugin.position, deleted, bool(flags & FLAG_PARTIAL), fields))
            elif tag in PLACED:
                base = pos = None
                if not deleted:
                    for t, d in subrecords(data_of(body, size, flags)):
                        if t == b'NAME' and len(d) >= 4:
                            base = key(struct.unpack_from('<I', d)[0])
                        elif t == b'DATA' and len(d) >= 12:
                            pos = struct.unpack_from('<3f', d)
                placed[k].append((plugin.position, cell, deleted, tag.decode(), base, pos))
            elif tag == b'NAVM':
                navm[k].append((plugin.position, cell, deleted))

    i = plugin.header_end
    while i < len(buf):
        gsize = struct.unpack_from('<I', buf, i + 4)[0]
        walk_group(i + 24, i + gsize, None)
        i += gsize


def main():
    ap = argparse.ArgumentParser(description="Fold conformance oracle (LMC-REC-002, -004, -007, -011).")
    ap.add_argument('--game', required=True, choices=['fo3', 'fnv', 'sse', 'fo4'])
    ap.add_argument('--plugins', required=True, help='file: one plugin path per line, in load order, masters first')
    ap.add_argument('--binary', required=True, help='the byroredux test binary')
    ap.add_argument('--work', help='directory for the dump (default: a temporary one)')
    ap.add_argument('--examples', type=int, default=3)
    args = ap.parse_args()

    paths = [l.strip() for l in open(args.plugins) if l.strip()]
    try:
        plugins = [Plugin(p, args.game, n) for n, p in enumerate(paths)]
    except (OSError, ValueError) as e:
        print('lmc-fold-oracle: %s' % e, file=sys.stderr)
        return 2
    resolver = Resolver(plugins)
    carriers = defaultdict(list)  # key -> [(position, type, deleted)]
    cells, placed, navm = defaultdict(list), defaultdict(list), defaultdict(list)
    for p in plugins:
        records = {}
        walk(p, resolver.of(p), records, cells, placed, navm)
        for k, (tag, deleted) in records.items():
            carriers[k].append((p.position, tag, deleted))
        p.buf = b"" # release the bytes once walked

    work = args.work or tempfile.mkdtemp(prefix='lmc-fold-')
    os.makedirs(work, exist_ok=True)
    expect = os.path.join(work, 'expect.txt')
    with open(expect, 'w') as f:
        for k, c in carriers.items():
            if k[0] == 'X' or k[0] < 0:
                continue
            if c[-1][2]:
                f.write('D %d %x\n' % k)
            elif len(c) > 1 or c[-1][0] != k[0]:
                f.write('W %d %x\n' % k)
    env = dict(os.environ, BYRO_FOLD_PLUGINS=os.path.abspath(args.plugins), BYRO_FOLD_EXPECT=expect, BYRO_FOLD_OUT=work)
    run = subprocess.run([args.binary, '--ignored', '--exact', TEST, '--nocapture'], env=env, capture_output=True, text=True)
    if run.returncode != 0:
        sys.stderr.write(run.stdout[-4000:] + run.stderr[-4000:])
        print('lmc-fold-oracle: the fold dump failed', file=sys.stderr)
        return 2
    fold = json.load(open(os.path.join(work, 'fold.json')))
    names = fold['plugins']
    if names != [p.name for p in plugins]:
        print('lmc-fold-oracle: the fold reordered the plugins (masters first?): %s' % names, file=sys.stderr)
        return 2

    # An out-of-range index passes through the engine unchanged: its top byte names whichever plugin holds that
    # regular slot.
    regular = {int(s[1:]): n for n, s in enumerate(fold['slots']) if s.startswith('R')}
    oor = lambda k: isinstance(k, tuple) and k[0] == 'X'
    engine_key = lambda k: (regular.get(k[1] >> 24, -1), k[1] & 0xFFFFFF) if oor(k) else k
    name = lambda pos: names[pos] if 0 <= pos < len(names) else '?'
    fmt = lambda k: 'unresolvable %08X' % k[1] if oor(k) else '%s:%06X' % (name(k[0]), k[1]) if k else 'none'

    failures, known = defaultdict(list), defaultdict(list)
    aliased = {engine_key(k) for k in carriers if oor(k)}
    for k, chain in carriers.items():
        if oor(k):
            known['form with an unresolvable FormID, read as another plugin\'s'].append(
                (k, [k, chain[-1][1].decode('latin-1'), 'aliased to', fmt(engine_key(k))]))

    def finding(what, k, detail):
        # A disagreement on a form an unresolvable id was aliased onto is that alias's consequence (#7329).
        collided = k in aliased or any(isinstance(x, tuple) and x in aliased for x in detail)
        (known if oor(k) or any(oor(x) for x in detail) or collided else failures)[what].append((k, detail))

    # Placed references, per merged cell.
    engine_refs = defaultdict(list)
    cell_count = 0
    for line in open(os.path.join(work, 'cells.jsonl')):
        c = json.loads(line)
        cell_count += 1
        ck = tuple(c['cell'])
        for p, o, bp, bo, x, y, z in c['refs']:
            engine_refs[(p, o)].append((ck, (bp, bo), (x, y, z), c['where']))
    ref_stats = Counter()
    seen = set()
    for k, chain in placed.items():
        win = chain[-1]
        ek = engine_key(k)
        seen.add(ek)
        got = engine_refs.get(ek, [])
        ref_stats['placed'] += 1
        ref_stats['overridden'] += len(chain) > 1
        ref_stats['moved between cells'] += len({c[1] for c in chain}) > 1
        if win[2]:
            ref_stats['deleted by the winner'] += 1
            if got:
                finding('reference kept although its winner deletes it', k, [k])
            continue
        if not got:
            finding('reference missing', k, [k])
            continue
        if len(got) > 1:
            finding('reference placed in %d cells' % len(got), k, [k])
        if all(g[0] != engine_key(win[1]) for g in got):
            finding('reference in the wrong cell', k, [k, win[1]])
        g = got[0]
        if oor(win[4]):
            known['reference whose base FormID is unresolvable'].append((k, [k, win[4], 'read as', fmt(engine_key(win[4]))]))
        if win[4] is not None and tuple(g[1]) != engine_key(win[4]):
            finding("reference with a loser's base form", k, [k, win[4]])
        elif win[5] is not None and any(abs(a - b) > 0.05 for a, b in zip(g[2], win[5])):
            finding("reference at a loser's position", k, [k])
    for ek in engine_refs:
        if ek not in seen and ek not in aliased:
            finding('reference no plugin places', ek, [ek])

    # Navmeshes.
    engine_navm = {}
    for line in open(os.path.join(work, 'navmeshes.txt')):
        p, o, cp, co = map(int, line.split())
        engine_navm[(p, o)] = None if cp == -2 else (cp, co)
    nav_stats = Counter()
    for k, chain in navm.items():
        win, ek = chain[-1], engine_key(k)
        nav_stats['navmeshes'] += 1
        if win[2]:
            nav_stats['deleted by the winner'] += 1
            if ek in engine_navm:
                finding('navmesh kept although its winner deletes it', k, [k])
        elif ek not in engine_navm:
            finding('navmesh missing', k, [k])
        elif engine_navm[ek] is not None and win[1] is not None and engine_navm[ek] != engine_key(win[1]):
            finding('navmesh under the wrong cell', k, [k, win[1]])

    # Every form: present with its type when the winner carries it, absent when the winner deletes it.
    engine_types = {}
    for line in open(os.path.join(work, 'record_types.txt')):
        p, o, t = line.split()
        engine_types[(int(p), int(o))] = t
    walked = set(engine_types.values())
    unwalked = Counter()
    form_stats = Counter()
    expected_keys = set()
    for k, chain in carriers.items():
        _, tag, deleted = chain[-1]
        tag = tag.decode('latin-1')
        ek = engine_key(k)
        expected_keys.add(ek)
        form_stats['forms'] += 1
        form_stats['overridden'] += len(chain) > 1
        if deleted:
            form_stats['deleted by the winner'] += 1
            if ek in engine_types:
                finding('form kept although its winner deletes it', k, [k, tag])
        elif tag not in walked:
            unwalked[tag] += 1
        elif ek not in engine_types:
            finding('form missing from the fold', k, [k, tag])
        elif engine_types[ek] != tag:
            finding('form held as another record type', k, [k, tag, engine_types[ek]])
    for ek, t in engine_types.items():
        if ek not in expected_keys and ek[0] >= 0 and ek not in aliased:
            finding('form no plugin carries', ek, [ek, t])

    # The fold's provenance winner (LMC-REC-007).
    winner_count, unwalked_winners = 0, Counter()
    for line in open(os.path.join(work, 'winners.txt')):
        p, o, w = line.split()
        k = (int(p), int(o, 16))
        last, tag = carriers[k][-1][0], carriers[k][-1][1].decode('latin-1')
        if tag not in walked:
            # A group the parser skips is never traced, so provenance can only name the slot owner.
            unwalked_winners[tag] += 1
            continue
        winner_count += 1
        if int(w) != last:
            finding('provenance names another winner', k, [k, name(int(w)), name(last)])

    # Tombstones a record category still holds.
    for category, n in fold['tombstones_kept'].items():
        failures['tombstoned forms still held by category %s' % category].append((None, [n]))

    # Informational: CELL fields an intermediate override changed that the winning override drops.
    lost = Counter()
    for k, chain in cells.items():
        if len(chain) < 3:
            continue
        base, win = chain[0][3], chain[-1][3]
        for mid in chain[1:-1]:
            for tag in set(mid[3]) | set(base):
                v0, vi, vw = base.get(tag), mid[3].get(tag), win.get(tag)
                if vi != v0 and vw != vi:
                    lost[tag.decode()] += 1

    print('fold conformance: %s, %d plugins, fold %.2f s' % (args.game, len(plugins), fold['fold_s']))
    print('  forms: %s' % ', '.join('%s %d' % kv for kv in form_stats.items()))
    print('  references: %s; %d merged cells' % (', '.join('%s %d' % kv for kv in ref_stats.items()), cell_count))
    print('  navmeshes: %s' % ', '.join('%s %d' % kv for kv in nav_stats.items()))
    print('  provenance winners checked: %d%s' % (winner_count, '; not checked in groups the parser skips: %s'
          % ', '.join('%s %d' % kv for kv in sorted(unwalked_winners.items())) if unwalked_winners else ''))
    if unwalked:
        print('  record types the parser does not walk (not checked): %s' % ', '.join('%s %d' % kv for kv in sorted(unwalked.items())))
    if lost:
        print('  CELL fields an intermediate override changed and the winner drops (game rule, informational): %s'
              % ', '.join('%s %d' % kv for kv in lost.most_common()))
    for title, table in (('known (out-of-range mod index, #7329)', known), ('FAIL', failures)):
        for what, items in sorted(table.items(), key=lambda kv: -len(kv[1])):
            print('  %s: %s: %d' % (title, what, len(items) if items[0][0] is not None else items[0][1][0]))
            for k, detail in items[:args.examples]:
                if k is not None:
                    print('      %s  %s' % (fmt(k), ' '.join(fmt(d) if isinstance(d, tuple) else str(d) for d in detail[1:])))
    if failures:
        print('fold conformance: FAIL (%d kinds of disagreement)' % len(failures))
        return 1
    print('fold conformance: ok')
    return 0


if __name__ == '__main__':
    sys.exit(main())

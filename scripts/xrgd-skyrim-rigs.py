#!/usr/bin/env python3
"""#6068 / #6069 — which Skyrim skeletons can an `XRGD` corpse pose be applied to?

`ragdoll_pose.rs` poses a Skyrim corpse by entry ORDER: entry k poses the
skeleton NIF's k-th `bhkRigidBody` (#5015, derived on the 18-body humanoid).
This script repeats #5015's two checks for any other rig, so a rig is added to
the gate only on its own evidence:

  offsets  Each entry's offset length equals its body's bone length. Reported
           as the median relative error per body (bones >= 1 unit long)
           against three references: the NIF bone's own local offset
           (`nif`), the ragdoll hkaSkeleton's bone (`hkx`, the second skeleton
           in skeleton.hkx, bones named `Ragdoll_<bone>`), and the rest
           distance from the parent body's bone (`rest`). Also per entry: which
           body's bone length the entry's offset matches best (`argmin`) — an
           order check that does not assume the order.
  FK       Forward kinematics of the corpse on the real NIF bone tree under all
           24 Euler conventions, ranked by the median vertical extent of the
           ragdoll bones (how flat the lying body is); the verified humanoid
           ranks intrinsic XYZ with negated angles (mode 0) first. Conventions
           that give the same rotation tie, and a tie for first counts. Also
           reported: the median depth of the lowest bone under the COM.

The flatness test discriminates weakly for a quadruped (a body lying on its
side is about as tall as one standing), so a failure there is "not
established", not "wrong".

Inputs (tools in this repo; one file per rig name):
  poses.jsonl          cargo run -p byroredux-plugin --example xrgd_dump -- Skyrim.esm
  <rigs>/rd/<rig>.txt   cargo run -p byroredux-nif --example ragdoll_dump -- <skeleton.nif>
  <rigs>/tree/<rig>.jsonl  cargo run -p byroredux-nif --example skeleton_tree_dump -- <skeleton.nif>
  <rigs>/hkx/<rig>.txt  cargo run -p byroredux-hkx --example inspect -- skeletons <skeleton.hkx>  (optional)

Usage: xrgd-skyrim-rigs.py <poses.jsonl> <rigs-dir> <rig>=<Race>[,<Race>..] [..]
       (append `:<n>` to a rig to analyse only poses with n body entries; default:
        the body count)
"""
import collections
import itertools
import json
import os
import re
import sys

import numpy as np
from scipy.spatial.transform import Rotation as R

POSES, RIGS_DIR = sys.argv[1:3]
SPECS = sys.argv[3:]
OURS = ('xyz', True, -1)  # intrinsic XYZ, negated angles
MAX_FK_CORPSES = 80

rows = [json.loads(l) for l in open(POSES)]
dead = [r for r in rows if r['dead']]


def is_marker(e):
    return (e[0] == 0 and abs(e[1]) < 1e-4 and (abs(e[2]) >= 1e-4 or abs(e[3]) >= 1e-4)
            and all(abs(v) < 1e-4 for v in e[4:7]))


def strip_marker(entries):
    return entries[:-1] if entries and is_marker(entries[-1]) else entries


def load_rig(rig):
    bodies, parent_body = [], {}
    for line in open(f'{RIGS_DIR}/rd/{rig}.txt'):
        m = re.match(r'#(\d+) "([^"]+)"', line)
        if m:
            bodies.append(m.group(2))
        m = re.match(r'joint#\d+ a=(\d+) b=(\d+)', line)
        if m:
            parent_body[int(m.group(1))] = int(m.group(2))
    tree, order = {}, []
    for line in open(f'{RIGS_DIR}/tree/{rig}.jsonl'):
        d = json.loads(line)
        if d['name'] not in tree:
            tree[d['name']] = d
            order.append(d['name'])
    hkx = {}
    path = f'{RIGS_DIR}/hkx/{rig}.txt'
    if os.path.exists(path):
        for line in open(path):
            m = re.match(r'1\s+\d+\s+"Ragdoll_(.*?)".*translation: \[(.*?)\]', line)
            if m:
                hkx[m.group(1).lower()] = np.array([float(x) for x in m.group(2).split(',')])
    return bodies, parent_body, tree, order, hkx


def rest_world(tree, order):
    pos, rot = {}, {}
    for name in order:
        d = tree[name]
        t, r = np.array(d['t']), np.array(d['r'])
        p = d['parent']
        if p in rot:
            pos[name], rot[name] = pos[p] + rot[p] @ t, rot[p] @ r
        else:
            pos[name], rot[name] = t, r
    return pos


def lying_pose(entries, bones, tree, order, seq, intrinsic, sign):
    rot, pos, override = {}, {}, {}
    for k, e in enumerate(entries):
        eul = np.array(e[4:7]) * sign
        ang = [eul['xyz'.index(a)] for a in seq]
        override[bones[k]] = R.from_euler(seq.upper() if intrinsic else seq, ang).as_matrix()
    for name in order:
        d = tree[name]
        t, r = np.array(d['t']), np.array(d['r'])
        if name in override:
            r = override[name]
        if name == bones[0]:
            t = np.zeros(3)  # the COM sits at the placement
        p = d['parent']
        if p in rot:
            pos[name], rot[name] = pos[p] + rot[p] @ t, rot[p] @ r
        else:
            pos[name], rot[name] = t, r
    return pos


def rel(a, b):
    return abs(a - b) / b if b >= 1.0 else None


for spec in SPECS:
    rig, races = spec.split('=')
    want = None
    if ':' in rig:
        rig, want = rig.split(':')
        want = int(want)
    races = set(races.split(','))
    bodies, parent_body, tree, order, hkx = load_rig(rig)
    want = want or len(bodies)
    corpses = [r for r in dead if r['race'] in races]
    counts = collections.Counter(len(strip_marker(r['entries'])) for r in corpses)
    use = [r for r in corpses if len(strip_marker(r['entries'])) == want]
    print(f'\n=== {rig}: {len(bodies)} bodies; {len(corpses)} corpses of {sorted(races)}; '
          f'entry counts after a marker {dict(sorted(counts.items()))}; analysing {len(use)} with {want}')
    if not use:
        continue
    missing = [b for b in bodies if b not in tree]
    if missing:
        print('  bodies missing from the NIF tree:', missing)
        continue
    rest = rest_world(tree, order)
    lengths = {
        b: {
            'nif': float(np.linalg.norm(tree[b]['t'])),
            'hkx': float(np.linalg.norm(hkx[b.lower()])) if b.lower() in hkx else None,
            'rest': float(np.linalg.norm(rest[b] - rest[bodies[parent_body[k]]])) if k in parent_body else None,
        }
        for k, b in enumerate(bodies)
    }
    print(f'  {"k":>2} {"body":28} {"entry |off|":>11} {"nif":>8} {"err":>6} {"hkx":>8} {"err":>6} {"rest":>8} {"err":>6}  argmin(nif)')
    passed = {'nif': [0, 0], 'hkx': [0, 0], 'rest': [0, 0]}
    for k in range(min(want, len(bodies))):
        off = float(np.median([np.linalg.norm(strip_marker(r['entries'])[k][1:4]) for r in use]))
        name = bodies[k]
        cells = []
        for ref in ('nif', 'hkx', 'rest'):
            L = lengths[name][ref]
            if L is None:
                cells.append(f'{"-":>8} {"-":>6}')
                continue
            errs = [rel(float(np.linalg.norm(strip_marker(r['entries'])[k][1:4])), L) for r in use]
            errs = [e for e in errs if e is not None]
            if errs and k > 0:
                m = float(np.median(errs))
                passed[ref][1] += 1
                passed[ref][0] += m <= 0.03
                cells.append(f'{L:8.3f} {m:6.1%}')
            else:
                cells.append(f'{L:8.3f} {"-":>6}')
        best = min(range(len(bodies)), key=lambda b: abs(lengths[bodies[b]]['nif'] - off))
        print(f'  {k:>2} {name:28} {off:11.3f} ' + ' '.join(cells) + f'  {best} {bodies[best]}')
    print('  offsets within 3 % (bones >= 1 unit): ' + ', '.join(f'{ref} {p}/{n}' for ref, (p, n) in passed.items()))
    if want != len(bodies):
        continue
    results = []
    sample = use[:MAX_FK_CORPSES]
    for seq in map(''.join, itertools.permutations('xyz')):
        for intrinsic in (False, True):
            for sign in (1, -1):
                ext, low = [], []
                for c in sample:
                    pos = lying_pose(strip_marker(c['entries']), bodies, tree, order, seq, intrinsic, sign)
                    z = np.array([pos[b][2] for b in bodies])
                    ext.append(z.max() - z.min())
                    low.append(-z.min())
                results.append((float(np.median(ext)), float(np.median(low)), (seq, intrinsic, sign)))
    results.sort()
    ours = next(x for x in results if x[2] == OURS)
    rank = 1 + sum(1 for e, _, _ in results if e < ours[0] - 1e-6)
    runner = next(x for x in results if abs(x[0] - ours[0]) > 1e-6)
    upright = []
    for c in sample:
        z = np.array([rest[b][2] for b in bodies])
        upright.append(z.max() - z.min())
    print(f'  FK over {len(sample)} corpses: ours (intrinsic XYZ, negated) rank {rank}/24, median extent '
          f'{ours[0]:.1f}, lowest bone {ours[1]:.1f} under the COM; best other {runner[0]:.1f} {runner[2]}; '
          f'rest-pose extent {np.median(upright):.1f}')
    for e, lo, conv in results[:4]:
        print(f'    extent {e:6.1f} low {lo:6.1f} {conv}')

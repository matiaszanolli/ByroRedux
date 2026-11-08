#!/usr/bin/env python3
"""Print the fog term of a Gamebryo shader package (.sdp) from its D3D9 bytecode.

VLM-HF-001 (docs/prd/volumetric-lighting.md) asks for the exact form of the authored fog power curve.
Fallout: New Vegas ships loose `Data/Shaders/shaderpackage*.sdp` files holding D3D9 shader bytecode
(vs_2_0 / ps_2_x) with a constant table. This reader walks each package for shaders whose constant table
names a `Fog*` constant, histograms the opcodes they use, and disassembles one small vertex shader so the
fog arithmetic can be read directly.

Usage: scripts/sdp-fog-term.py <shaderpackageNNN.sdp> [max-instructions-to-print-for-the-sample, default 30]

Read-only; untrusted input (run with `python3 -I`). Only the opcodes the fog term needs are named.
"""
import collections
import struct
import sys

OPS = {0: 'nop', 1: 'mov', 2: 'add', 3: 'sub', 4: 'mad', 5: 'mul', 6: 'rcp', 7: 'rsq', 8: 'dp3', 9: 'dp4',
       10: 'min', 11: 'max', 12: 'slt', 13: 'sge', 14: 'exp', 15: 'log', 16: 'lit', 17: 'dst', 18: 'lrp',
       19: 'frc', 20: 'm4x4', 21: 'm4x3', 22: 'm3x4', 23: 'm3x3', 24: 'm3x2', 25: 'call', 26: 'callnz',
       27: 'loop', 28: 'ret', 29: 'endloop', 30: 'label', 31: 'dcl', 32: 'pow', 33: 'crs', 34: 'sgn',
       35: 'abs', 36: 'nrm', 37: 'sincos', 38: 'rep', 39: 'endrep', 40: 'if', 41: 'ifc', 42: 'else',
       43: 'endif', 44: 'break', 45: 'breakc', 46: 'mova', 64: 'texcoord', 65: 'texkill', 66: 'texld',
       81: 'def', 82: 'defi', 83: 'defb', 84: 'seq', 85: 'sne', 86: 'dsx', 87: 'dsy', 88: 'setp',
       89: 'texldl', 90: 'breakp'}
VERSIONS = {0xFFFE0200: 'vs_2_0', 0xFFFE0101: 'vs_1_1', 0xFFFE0300: 'vs_3_0', 0xFFFF0200: 'ps_2_0',
            0xFFFF0201: 'ps_2_x', 0xFFFF0300: 'ps_3_0', 0xFFFF0101: 'ps_1_1', 0xFFFF0104: 'ps_1_4'}
REG_TYPES = {0: 'r', 1: 'v', 2: 'c', 3: 'a', 5: 'oD', 6: 'oT', 7: 'cb', 8: 'i', 9: 'b', 15: 's'}
MAX_INSTRUCTIONS = 400


def parse_shader(data, off):
    """Return (instructions, ctab bytes or None, end offset), or None when the stream is not a shader."""
    pos, ins, ctab = off + 4, [], None
    while pos + 4 <= len(data):
        token = struct.unpack_from('<I', data, pos)[0]
        op = token & 0xFFFF
        if op == 0xFFFF:
            return ins, ctab, pos + 4
        if op == 0xFFFE:  # comment: the constant table lives here
            words = (token >> 16) & 0x7FFF
            body = data[pos + 4:pos + 4 + 4 * words]
            if body[:4] == b'CTAB':
                ctab = body
            pos += 4 + 4 * words
            continue
        if op not in OPS:
            return None
        length = (token >> 24) & 0xF
        if pos + 4 + 4 * length > len(data):
            return None
        ins.append((op, struct.unpack_from('<%dI' % length, data, pos + 4) if length else ()))
        pos += 4 + 4 * length
        if len(ins) > MAX_INSTRUCTIONS:
            return None
    return None


def constants(ctab):
    body = ctab[4:]
    count, info = struct.unpack_from('<2I', body, 12)
    out = []
    for i in range(count):
        name_off, _regset, reg, regcount = struct.unpack_from('<IHHH', body, info + i * 20)
        end = body.index(b'\0', name_off)
        out.append((body[name_off:end].decode('latin1'), reg, regcount))
    return out


def register(token, dest):
    kind = ((token >> 28) & 7) | ((token >> 8) & 0x18)
    num = token & 0x7FF
    if kind == 4:
        text = ['oPos', 'oFog', 'oPts'][num] if num < 3 else 'rast%d' % num
    else:
        text = '%s%d' % (REG_TYPES.get(kind, '?%d' % kind), num)
    if dest:
        mask = (token >> 16) & 0xF
        return text + ('' if mask == 0xF else '.' + ''.join(c for i, c in enumerate('xyzw') if mask >> i & 1))
    swizzle = (token >> 16) & 0xFF
    sw = ''.join('xyzw'[(swizzle >> (2 * i)) & 3] for i in range(4))
    text += '' if sw == 'xyzw' else '.' + sw
    return '-' + text if (token >> 24) & 0xF == 1 else text


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    limit = int(sys.argv[2]) if len(sys.argv) > 2 else 30
    data = open(sys.argv[1], 'rb').read()
    shaders, i = [], 0
    while i < len(data) - 8:
        version = struct.unpack_from('<I', data, i)[0]
        parsed = parse_shader(data, i) if version in VERSIONS else None
        if parsed and parsed[0] and parsed[1]:
            try:
                shaders.append((i, VERSIONS[version], parsed[0], constants(parsed[1])))
            except (struct.error, ValueError):
                pass
            i = parsed[2]
            continue
        i += 4
    fog = [s for s in shaders if any('Fog' in c[0] for c in s[3])]
    print(f'{len(shaders)} shaders with a constant table, {len(fog)} naming a Fog* constant')
    hist = collections.Counter()
    for _, version, ins, _ in fog:
        used = {OPS[o] for o, _ in ins}
        hist[(version, 'pow' in used, 'exp' in used and 'log' in used)] += 1
    for (version, has_pow, has_explog), count in sorted(hist.items()):
        print(f'  {version}: {count:4d} shaders; pow={has_pow} exp+log={has_explog}')
    # The fog sequence: how many exp+log vertex shaders share one instruction pattern around the log.
    patterns = collections.Counter()
    for _, version, ins, cs in fog:
        ops = [o for o, _ in ins]
        if version == 'vs_2_0' and 14 in ops and 15 in ops and any(c[0] == 'FogParam' for c in cs):
            k = ops.index(15)
            patterns[tuple(OPS[o] for o in ops[max(0, k - 6):k + 3])] += 1
    for pattern, count in patterns.most_common(5):
        print(f'  {count:4d} vs_2_0 shaders share: {" ".join(pattern)}')
    for off, version, ins, cs in fog:
        ops = {o for o, _ in ins}
        if version == 'vs_2_0' and {14, 15} <= ops and len(ins) <= limit:
            print(f'\nsample vs_2_0 @{off}, {len(ins)} instructions')
            for name, reg, count in cs:
                print(f'  const {name:14s} c{reg} x{count}')
            for op, toks in ins:
                if op == 81:
                    print('  def', register(toks[0], True), [struct.unpack('<f', struct.pack('<I', t))[0] for t in toks[1:]])
                elif op == 31:
                    print('  dcl', register(toks[1], True))
                elif toks:
                    print(' ', OPS[op], register(toks[0], True), ', '.join(register(t, False) for t in toks[1:]))
                else:
                    print(' ', OPS[op])
            break


if __name__ == '__main__':
    main()

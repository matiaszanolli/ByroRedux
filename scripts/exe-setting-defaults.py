#!/usr/bin/env python3
"""Read game-setting (GMST / INI) defaults from a Bethesda game executable.

A master authors only the settings it changes; every other setting takes the
default compiled into the executable. This reads those defaults, so a settings
census can report a value for a setting no master authors (M65 Phase 0, #6909;
docs/engine/sneak-detection.md, Appendix A).

Two storage forms are recognised:

* **Static** (Fallout3.exe, SkyrimSE.exe, Fallout4.exe): the setting object sits
  in `.data`, its default value immediately before a pointer to its name
  string. Value width: 4 bytes read; the pointer is 4 or 8 bytes (PE32 / PE32+).
* **Initializer** (Oblivion.exe, 32-bit only): code builds the object at start
  up, `fld [const] | fldz | fld1; push ecx; fstp [esp]; [mov ecx, obj;] push
  <name>` for a float, `push <imm32>; push <name>` for an integer. Needs python
  `capstone`.

* **Initializer64** (Starfield.exe, 64-bit): each setting has its own initializer,
  `vmovss xmm2, [default]` (or `vxorps xmm2, xmm2, xmm2` for 0), `lea rdx, name`,
  `lea rcx, object`, `call <registrar>` (`0x1402d65e0` in the Steam build), with no
  static object. An integer setting loads `mov r8d, imm32` instead and calls its own
  registrar (`0x1402d66a0`). The setting is found from its name alone: the `lea rdx`
  that points at the name string, followed by `lea rcx` and an `E8` call.

FalloutNV.exe's `.text` is SteamStub-encrypted and its settings are not static,
so neither form finds them; the tool reports `<not found>`.

Usage: exe-setting-defaults.py <exe> <name> [<name> ...]
Prints `<name>\\t<value>` per name; the value is `<not found>` when neither form
resolves it. Integer settings (`i`, `u` prefix) print as integers, the rest as
floats.
"""
import struct
import sys


def pe_sections(pe):
    off = struct.unpack_from('<I', pe, 0x3c)[0]
    count = struct.unpack_from('<H', pe, off + 6)[0]
    opt_size = struct.unpack_from('<H', pe, off + 20)[0]
    is64 = struct.unpack_from('<H', pe, off + 24)[0] == 0x20b
    base = struct.unpack_from('<Q' if is64 else '<I', pe, off + 24 + (24 if is64 else 28))[0]
    sections = {}
    for k in range(count):
        s = off + 24 + opt_size + k * 40
        name = pe[s:s + 8].rstrip(b'\0').decode('latin1')
        _vsize, va, rsize, raw = struct.unpack_from('<IIII', pe, s + 8)
        sections[name] = (va, raw, rsize)
    return is64, base, sections


def fmt(name, raw4):
    if name[:1] in ('i', 'u'):
        return str(struct.unpack('<i', raw4)[0])
    return '%g' % struct.unpack('<f', raw4)[0]


def main():
    exe, names = sys.argv[1], sys.argv[2:]
    pe = open(exe, 'rb').read()
    is64, base, secs = pe_sections(pe)

    def offset_to_va(o):
        for va, raw, rsize in secs.values():
            if raw <= o < raw + rsize:
                return base + va + o - raw
        return None

    def read_va(v):
        for va, raw, rsize in secs.values():
            if base + va <= v < base + va + rsize:
                o = raw + v - base - va
                return pe[o:o + 4]
        return None

    lea_index = {}
    md = None
    if not is64 and '.text' in secs:
        try:
            from capstone import Cs, CS_ARCH_X86, CS_MODE_32
            md = Cs(CS_ARCH_X86, CS_MODE_32)
        except ImportError:
            md = None

    for name in names:
        i = pe.find(b'\0' + name.encode() + b'\0')
        sva = offset_to_va(i + 1) if i >= 0 else None
        value = None
        if sva is not None:
            value = static_form(pe, secs, is64, sva, name)
            if value is None and is64 and '.text' in secs:
                value = initializer64_form(pe, secs, base, sva, name, read_va, lea_index)
            if value is None and md is not None:
                value = initializer_form(pe, secs, base, md, sva, name, read_va)
        print(f'{name}\t{value if value is not None else "<not found>"}')


def static_form(pe, secs, is64, sva, name):
    psize = 8 if is64 else 4
    ptr = struct.pack('<Q' if is64 else '<I', sva)
    for sec in ('.data', '.rdata'):
        if sec not in secs:
            continue
        _va, raw, rsize = secs[sec]
        blob = pe[raw:raw + rsize]
        k = blob.find(ptr)
        while k >= 0:
            if k % psize == 0 and k >= psize:
                return fmt(name, blob[k - psize:k - psize + 4])
            k = blob.find(ptr, k + 1)
    return None


def initializer64_form(pe, secs, base, sva, name, read_va, index):
    """Starfield: `[vmovss xmm2,[rip+d] | vxorps xmm2,xmm2,xmm2]; lea rdx,[name]; lea rcx,[obj]; call`.

    `index` caches every `lea rdx, [rip+d]` target of `.text` -> the instruction addresses, built once.
    """
    tva, traw, trsize = secs['.text']
    text = pe[traw:traw + trsize]
    if not index:
        index[None] = True
        k = text.find(b'\x48\x8d\x15')
        while k >= 0:
            if k + 7 <= len(text):
                at = base + tva + k
                target = at + 7 + struct.unpack_from('<i', text, k + 3)[0]
                index.setdefault(target, []).append(k)
            k = text.find(b'\x48\x8d\x15', k + 1)
    for k in index.get(sva, []):
        # lea rcx,[rip+d] (7 bytes) then an E8 call (5 bytes) must follow the lea rdx
        if text[k + 7:k + 10] != b'\x48\x8d\x0d' or text[k + 14:k + 15] != b'\xe8':
            continue
        pre = text[max(0, k - 8):k]
        if text[k - 6:k - 4] == b'\x41\xb8':  # mov r8d, imm32: an integer setting (its own registrar)
            return fmt(name, text[k - 4:k])
        if pre[-4:] == b'\xc5\xe8\x57\xd2':  # vxorps xmm2,xmm2,xmm2
            return '0'
        if len(pre) == 8 and pre[:4] == b'\xc5\xfa\x10\x15':  # vmovss xmm2,[rip+d]
            at = base + tva + k  # the vmovss ends where the lea rdx begins
            raw4 = read_va(at + struct.unpack('<i', pre[4:])[0])
            if raw4:
                return fmt(name, raw4)
    return None


def initializer_form(pe, secs, base, md, sva, name, read_va):
    tva, traw, trsize = secs['.text']
    text = pe[traw:traw + trsize]
    push_name = b'\x68' + struct.pack('<I', sva)
    k = text.find(push_name)
    while k >= 0:
        here = base + tva + k
        best = None
        for back in range(8, 25):
            window = text[max(0, k - back):k]
            ins = list(md.disasm(window, here - len(window)))
            if ins and ins[-1].address + ins[-1].size == here:
                best = [(x.mnemonic, x.op_str) for x in ins]
                if any(m in ('fld', 'fldz', 'fld1') for m, _ in best):
                    break
        if best:
            loads = [(m, op) for m, op in best if m in ('fld', 'fldz', 'fld1')]
            if loads and any(m == 'fstp' for m, _ in best):
                m, op = loads[-1]
                if m == 'fldz':
                    return '0'
                if m == 'fld1':
                    return '1'
                if '[' in op:
                    raw4 = read_va(int(op.split('[')[1].rstrip(']'), 16))
                    if raw4:
                        return fmt(name, raw4)
            m, op = best[-1]
            if m == 'push':
                try:
                    v = int(op, 16) if op.lstrip('-').startswith('0x') else int(op)
                except ValueError:
                    v = None
                if v is not None:
                    return fmt(name, struct.pack('<i', v) if v < 0 else struct.pack('<I', v & 0xffffffff))
        k = text.find(push_name, k + 1)
    return None


if __name__ == '__main__':
    main()

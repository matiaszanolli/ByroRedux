import struct, capstone
class PE64:
    def __init__(s, path):
        s.b = open(path,'rb').read()
        e = struct.unpack_from('<I', s.b, 0x3c)[0]
        nsec = struct.unpack_from('<H', s.b, e+6)[0]
        opt = struct.unpack_from('<H', s.b, e+20)[0]
        s.base = struct.unpack_from('<Q', s.b, e+24+24)[0]
        s.secs=[]; o=e+24+opt
        for i in range(nsec):
            name=s.b[o:o+8].rstrip(b'\0').decode('latin1'); vs,va,rs,ro=struct.unpack_from('<IIII',s.b,o+8)
            s.secs.append((name,va,vs,ro,rs)); o+=40
        s.md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64); s.md.detail=True; s.md.skipdata=True
    def va2off(s,v):
        for n,va,vs,ro,rs in s.secs:
            if s.base+va<=v<s.base+va+rs: return v-s.base-va+ro
    def off2va(s,o):
        for n,va,vs,ro,rs in s.secs:
            if ro<=o<ro+rs: return s.base+va+o-ro
    def cstr(s,v):
        o=s.va2off(v)
        if o is None: return None
        e=s.b.find(b'\0',o)
        if e-o>80: return None
        t=s.b[o:e]
        return t.decode('latin1') if t and all(32<=c<127 for c in t) else None
    def q(s,v):
        o=s.va2off(v); return None if o is None else struct.unpack_from('<Q',s.b,o)[0]
    def f(s,v):
        o=s.va2off(v); return None if o is None else struct.unpack_from('<f',s.b,o)[0]
    def setting(s,v):
        # value at v, name pointer at v+8
        p=s.q(v+8); n=s.cstr(p) if p else None
        if n and n[:1] in 'fiubs' and len(n)>2: return n
        return None
    def dis(s,va,n):
        o=s.va2off(va); return list(s.md.disasm(s.b[o:o+n],va))

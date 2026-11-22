# Find instructions in .text whose RIP-relative operand resolves to a setting's value.
import sys, struct; sys.path.insert(0,'.')
import numpy as np
from pe64 import PE64
from capstone.x86 import X86_OP_MEM, X86_REG_RIP
p=PE64(sys.argv[1])
n,va,vs,ro,rs=[s for s in p.secs if s[0]=='.text'][0]
t=np.frombuffer(p.b[ro:ro+rs],dtype=np.uint8); base=p.base+va
d=np.frombuffer(p.b[ro:ro+rs-3],dtype=np.uint8)  # placeholder
disp=(t[:-3].astype(np.int64) | (t[1:-2].astype(np.int64)<<8) | (t[2:-1].astype(np.int64)<<16) | (t[3:].astype(np.int64)<<24))
disp=np.where(disp>=2**31, disp-2**32, disp)
idx=np.arange(len(disp),dtype=np.int64)
names=sys.argv[2:]
# resolve setting names to value addresses: find name string, then pointer to it, value = ptr-8
def addr_of(name):
    s=p.b.find(b'\0'+name.encode()+b'\0')
    if s<0: return None
    sva=p.off2va(s+1); o=p.b.find(struct.pack('<Q',sva))
    return p.off2va(o)-8 if o>=0 else None
for name in names:
    a=addr_of(name)
    if a is None: print(name,'?'); continue
    hits=set()
    for k in (0,1,4):
        m=np.nonzero(base+idx+4+k+disp==a)[0]
        for i in m:
            # verify by disassembling back up to 10 bytes
            for back in range(2,11):
                st=base+int(i)-back
                ins=p.dis(st,32)
                if not ins or ins[0].address!=st: continue
                i0=ins[0]
                try: ops=i0.operands or []
                except Exception: continue
                ok=any(op.type==X86_OP_MEM and op.mem.base==X86_REG_RIP and i0.address+i0.size+op.mem.disp==a for op in ops)
                if ok: hits.add(i0.address); break
    print(name, hex(a), ' '.join(hex(h) for h in sorted(hits)))

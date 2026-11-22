import sys; sys.path.insert(0,'.')
from pe64 import PE64
import capstone
from capstone.x86 import X86_OP_MEM, X86_REG_RIP
p=PE64(sys.argv[1]); va=int(sys.argv[2],16); n=int(sys.argv[3],0)
for i in p.dis(va,n):
    note=''
    for op in getattr(i,'operands',[]) or []:
        if op.type==X86_OP_MEM and op.mem.base==X86_REG_RIP:
            a=i.address+i.size+op.mem.disp
            st=p.setting(a)
            if st: note=f'  ; {st} = {p.f(a):g}'
            else:
                c=p.cstr(a)
                if c: note=f'  ; "{c}"'
                else:
                    v=p.f(a); note=f'  ; [{a:#x}] f={v:g}' if v is not None else f'  ; [{a:#x}]'
    if i.mnemonic in ('call','jmp') and i.op_str.startswith('0x'):
        note+=''
    print(f'{i.address:#x} {i.mnemonic} {i.op_str}{note}')
    if i.mnemonic=="int3" and i.address>va: break

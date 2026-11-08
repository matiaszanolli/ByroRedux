import re,subprocess,sys,os
root='/mnt/data/src/gamebyro-redux'
files=subprocess.check_output(['rg','-l',r'SettingEntry::(toggle|slider|choice|text|number|[a-z_]+)\(','byroredux/src','crates','-g','*.rs'],cwd=root,text=True).split()
def split_args(s):
    args=[];depth=0;cur='';instr=False;i=0
    while i<len(s):
        c=s[i]
        if instr:
            cur+=c
            if c=='\\': cur+=s[i+1]; i+=1
            elif c=='"': instr=False
        else:
            if c=='"': instr=True;cur+=c
            elif c in '([{': depth+=1;cur+=c
            elif c in ')]}':
                if depth==0: args.append(cur.strip()); return args
                depth-=1;cur+=c
            elif c==',' and depth==0: args.append(cur.strip());cur=''
            else: cur+=c
        i+=1
    return args
rows=[]
for f in files:
    txt=open(os.path.join(root,f),encoding='utf-8').read()
    # skip test modules
    cut=txt.find('#[cfg(test)]')
    body=txt if cut<0 else txt[:cut]
    for m in re.finditer(r'SettingEntry::([a-z_]+)\(',body):
        a=split_args(body[m.end():])
        line=body[:m.start()].count('\n')+1
        rows.append((f,line,m.group(1),a))
for f,l,k,a in rows:
    print(f'{f}:{l}\t{k}\t'+'\t'.join(x.replace('\n',' ')[:90] for x in a[:4]))

import sys,re,csv,os
D=os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
def load_tsv(p, kcol=0, vcol=1, skip=0):
    out=[]
    for l in open(p,encoding='utf-8').read().splitlines()[skip:]:
        c=l.split('\t')
        if len(c)>max(kcol,vcol): out.append((c[kcol],c[vcol]))
    return out
src={
 'fo3_exe': load_tsv(os.path.join(D,'exe_table_fo3.tsv'),skip=1),
 'fo3_esm': [x for x in load_tsv(os.path.join(D,'gmst_fo3.tsv')) if x[0][:1]=='s'],
 'fnv_esm': [x for x in load_tsv(os.path.join(D,'gmst_fnv.tsv')) if x[0][:1]=='s'],
 'sky_tr': load_tsv(os.path.join(D,'tr/sky_en.tsv')),
 'fo4_tr': load_tsv(os.path.join(D,'tr/fo4_en.tsv')),
}
def norm(s): return re.sub(r'[^a-z0-9]+',' ',s.lower()).strip()
idx={}
for g,rows in src.items():
    for k,v in rows:
        idx.setdefault(norm(v),[]).append((g,k,v))
def find(word):
    return idx.get(norm(word),[])
if __name__=='__main__':
    for w in sys.argv[1:]:
        print(w,'->',[(g,k) for g,k,v in find(w)][:12])

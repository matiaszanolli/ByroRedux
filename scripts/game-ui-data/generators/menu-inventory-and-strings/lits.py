import sys,re
for p in sys.argv[1:]:
    lines=open(p,encoding='utf-8').read().split('\n')
    end=len(lines)
    for i,l in enumerate(lines):
        if l.strip().startswith('#[cfg(test)]') or l.strip().startswith('#[cfg(all(test'):
            end=i;break
    for i in range(end):
        l=lines[i]; s=l.strip()
        if s.startswith('//') or s.startswith('///') or s.startswith('//!'): continue
        if re.search(r'log::|tracing::|assert|panic!|expect\(|unwrap_or_else|debug_assert|#\[|Id::new|id_salt|include_str|concat!|env!|cfg!', l): continue
        for m in re.finditer(r'"((?:[^"\\]|\\.)*)"', l):
            t=m.group(1)
            if re.search(r'[A-Za-z]{2}', t):
                print(f'{p}:{i+1}: {t}')

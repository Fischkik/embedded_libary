"""Register demos: reg.py <field_mod> <path_id> "<id>:<mod>:<planned title>" ... ; glossary entries read from JSON on stdin."""
import re, sys, json
field, path_id = sys.argv[1], sys.argv[2]
items = [a.split(':', 2) for a in sys.argv[3:]]
p = 'crates/demos/src/lib.rs'
s = open(p).read()
for demo_id, mod, planned in items:
    # module line
    m = re.search(r'pub mod %s \{\n(.*?)\n\}' % field, s, re.S)
    if m:
        mods = sorted(set(m.group(1).split('\n') + ['    pub mod %s;' % mod]))
        s = s[:m.start(1)] + '\n'.join(mods) + s[m.end(1):]
    else:
        s = s.replace('pub fn registry() -> Registry {', 'pub mod %s {\n    pub mod %s;\n}\n\npub fn registry() -> Registry {' % (field, mod), 1)
    # registry entry: after the last entry of the same field, else at the end of the list
    entry = '            %s::%s::entry(),\n' % (field, mod)
    last = [m for m in re.finditer(r'            %s::\w+::entry\(\),\n' % field, s)]
    if last:
        pos = last[-1].end()
    else:
        pos = s.index('        ],\n        paths: vec![')
    s = s[:pos] + entry + s[pos:]
    # learning path
    m = re.search(r'id: "%s",.*?steps: &\[(.*?)\]' % path_id, s, re.S)
    assert m, path_id
    s = s[:m.end(1)] + (', ' if m.group(1).strip() else '') + '"%s"' % demo_id + s[m.end(1):]
    # planned
    if planned:
        q = '"%s"' % planned
        assert q in s, planned
        own_line = re.search(r'\n[ \t]*' + re.escape(q) + r',(?=\n)', s)
        s = s[:own_line.start()] + s[own_line.end():] if own_line else s.replace(q + ', ', '', 1) if (q + ', ') in s else s.replace(', ' + q, '', 1) if (', ' + q) in s else s.replace(q + ',\n', '', 1) if (q + ',\n') in s else s.replace(q, '', 1)
open(p, 'w').write(s)

g = json.load(sys.stdin)
if g:
    p = 'crates/lab-ui/src/glossary.rs'
    s = open(p).read()
    m = re.search(r'pub static GLOSSARY: &\[\(&str, &str\)\] = &\[\n(.*?)\n\];', s, re.S)
    lines = m.group(1).split('\n')
    have = {re.match(r'\s*\("([^"]+)"', l).group(1).lower() for l in lines}
    for k, v in g.items():
        assert '"' not in v and k.lower() not in have, k
        lines.append('    ("%s", "%s"),' % (k, v))
    lines.sort(key=lambda l: re.match(r'\s*\("([^"]+)"', l).group(1).lower())
    s = s[:m.start(1)] + '\n'.join(lines) + s[m.end(1):]
    open(p, 'w').write(s)
print('registered', [i[0] for i in items])

"""Record one finished demo in the handoff documents.

    python3 tools/mark_done.py <demo-id> "<title shown in the library>" "<README row name>" "<one-line note>"

Recounts demos, remaining planned topics and passing tests from the repository itself, then
updates HANDOFF.md (state numbers + the "Done since" list), the progress log in AGENTS.md,
and the table and counts in README.md. Run it after `cargo test --workspace` has passed and
the screenshot has been looked at.
"""
import re, subprocess, sys, datetime

demo_id, title, row, note = sys.argv[1:5]
lib = open('crates/demos/src/lib.rs').read()
demos = len(re.findall(r'::entry\(\),', lib))
body = re.search(r'fn planned\(\).*?\n\}', lib, re.S).group(0)
left = sum(len(re.findall(r'"[^"]+"', titles)) for _, titles in re.findall(r'\(\s*&\[(.*?)\],\s*&\[(.*?)\]\s*,?\s*\)', body, re.S))
out = subprocess.run('cargo test --workspace 2>&1 | grep "test result"', shell=True, capture_output=True, text=True).stdout
passed = sum(int(m) for m in re.findall(r'(\d+) passed', out))
failed = sum(int(m) for m in re.findall(r'(\d+) failed', out))
assert failed == 0 and passed > 0, out
today = datetime.date.today().isoformat()

s = open('HANDOFF.md').read()
s = re.sub(r'- `cargo test --workspace`: \*\*\d+ passed, 0 failed\*\*\.', f'- `cargo test --workspace`: **{passed} passed, 0 failed**.', s)
s = re.sub(r'- \*\*\d+ demos\*\* registered, \*\*(\d+) learning paths\*\*\. \d+ topics remain in `planned\(\)`\.', lambda m: f'- **{demos} demos** registered, **{m.group(1)} learning paths**. {left} topics remain in `planned()`.', s)
s = re.sub(r'## 3\. What is left: \d+ demos', f'## 3. What is left: {left} demos', s)
marker = '## 3. What is left'
if '### Done since this file was first written' not in s:
    s = s.replace(marker, '### Done since this file was first written\n\nSkip these in section 3: their briefs are kept only for reference.\n\n' + marker, 1)
head, tail = s.split(marker, 1)
head = head.rstrip('\n') + f'\n- {today}: `{demo_id}` - {title}. {note}\n\n'
open('HANDOFF.md', 'w').write(head + marker + tail)

s = open('AGENTS.md').read()
anchor = '\n## When the user asks "how does X work?"'
i = s.index(anchor)
s = s[:i].rstrip('\n') + f'\n- {today}: `{demo_id}` built ({passed} tests pass, screenshot looked at). {left} topics left. {note}\n' + s[i:]
open('AGENTS.md', 'w').write(s)

s = open('README.md').read()
s = re.sub(r'(learning paths, )\d+( demos:)', rf'\g<1>{demos}\g<2>', s)
s = re.sub(r'^\S+ more topics are planned', f'{left} more topics are planned', s, flags=re.M)
lines = s.split('\n')
for k, line in enumerate(lines):
    if line.startswith(f'| {row} |'):
        lines[k] = line.rstrip().rstrip('|').rstrip() + f' · {title} |'
        break
else:
    raise SystemExit(f'README row "{row}" not found')
open('README.md', 'w').write('\n'.join(lines))
print(f'{demo_id}: {demos} demos, {left} left, {passed} tests')

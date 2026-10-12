"""One-off: add `corrected: false` to Unit struct literals and make the field
skip-serialized when false, so segmentation hashes stay stable. Run from the
repository root: python3 tools/fix_unit_corrected.py"""
import re
import pathlib

root = pathlib.Path('.')
p = root / 'crates/cs-model/src/lib.rs'
s = p.read_text()
s = s.replace('    #[serde(default)]\n    pub corrected: bool,',
              '    #[serde(default, skip_serializing_if = "std::ops::Not::not")]\n    pub corrected: bool,', 1)
p.write_text(s)

for rel in ['crates/cs-store/src/lib.rs', 'crates/cs-ingest/src/segment.rs',
            'crates/cs-store/src/tests.rs', 'crates/cs-ai/src/tests.rs']:
    p = root / rel
    s = p.read_text()
    out = []
    i = 0
    n = 0
    while True:
        j = s.find('Unit {', i)
        if j < 0:
            out.append(s[i:])
            break
        k = s.find('}', j)
        block = s[j:k]
        if 'corrected' not in block and re.search(r'\n\s*text\b', block):
            block2 = re.sub(r'(\n(\s*)text\b[^\n]*\n)',
                            lambda m: m.group(1) + m.group(2) + 'corrected: false,\n',
                            block, count=1)
            if block2 != block:
                n += 1
            out.append(s[i:j] + block2)
        else:
            out.append(s[i:k])
        i = k
    p.write_text(''.join(out))
    print(rel, 'literals patched:', n)

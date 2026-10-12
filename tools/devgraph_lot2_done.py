"""One-off: mark lot 2 tasks green in the dev graph and append the log."""
import pathlib

p = pathlib.Path('docs/DEVGRAPH_lot2_2026-10-12.md')
s = p.read_text()
for tid in ['T12', 'T13', 'T14', 'T15', 'T16', 'T17', 'T18', 'T19', 'T20']:
    i = s.find('| ' + tid + ' |')
    if i >= 0:
        j = s.find('\n', i)
        row = s[i:j]
        s = s[:i] + row.replace('| todo |', '| green |') + s[j:]
s += """- 12.10 22:15 lot 2 green: cs-store 71, cs-app 11, cs-ai 26, cs-ingest 23, cs-model 9 tests; web 66 tests; clippy 0; web-dist rebuilt. Decisions recorded: D-011 (retroactive) and D-012 in the thesis Decision Log; reference method `03_Method/generated/2026-10-12_METHOD_reference_v4.md`.
- Choices made by implementers, to keep in mind: unattributable field codes listed under interview "" in the saturation record (C6); corrected units also overlay phase 1 rows; `current_unit` stays crate-private, cs-app overlays corrections itself; claim codes validated against the unit's current coding; first export of an interview draws the verification sample seed.
"""
p.write_text(s)
print('ok')

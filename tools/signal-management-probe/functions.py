"""Summarize compiler stats and CodeView procedures in one probe run.

Counts/lengths describe emitted object procedures, not linked code or RAM.
The widgets test and sigmut dependency are reported separately.
"""
import collections
import hashlib
import json
import mmap
import pathlib
import struct
import sys


def procedures(path):
    with path.open('rb') as file, mmap.mmap(file.fileno(), 0, access=mmap.ACCESS_READ) as data:
        big = data[:4] == b'\0\0\xff\xff'
        count = struct.unpack_from('<I' if big else '<H', data, 44 if big else 2)[0]
        start = 56 if big else 20 + struct.unpack_from('<H', data, 16)[0]
        for index in range(count):
            offset = start + index * 40
            if data[offset:offset + 8] != b'.debug$S':
                continue
            length, raw = struct.unpack_from('<II', data, offset + 16)
            cursor = raw + 4
            while cursor + 8 <= raw + length:
                kind, size = struct.unpack_from('<II', data, cursor)
                if kind == 0xf1:
                    record, end = cursor + 8, cursor + 8 + size
                    while record + 4 <= end:
                        record_size, leaf = struct.unpack_from('<HH', data, record)
                        if record_size < 2:
                            break
                        if leaf in (0x1110, 0x110f, 0x1146, 0x1147) and record_size > 37:
                            name = data[record + 39:record + record_size + 2].split(b'\0', 1)[0]
                            code = struct.unpack_from('<I', data, record + 16)[0]
                            yield name, code
                        record += record_size + 2
                cursor += 8 + ((size + 3) & ~3)


def objects(paths):
    families = {}
    total = collections.Counter()
    object_count = 0
    for path in paths:
        object_count += 1
        for name, code in procedures(path):
            total['records'] += 1
            total['code_bytes'] += code
            if not name.startswith(b'sigmut::'):
                continue
            family = name.split(b'<', 1)[0].decode(errors='replace')
            item = families.setdefault(family, dict(records=0, code_bytes=0, names=set()))
            item['records'] += 1
            item['code_bytes'] += code
            item['names'].add(hashlib.blake2b(name, digest_size=16).digest())
    return dict(object_count=object_count, total=dict(total), families=[
        dict(name=name, records=item['records'], code_bytes=item['code_bytes'], unique_names=len(item['names']))
        for name, item in sorted(families.items(), key=lambda pair: -pair[1]['code_bytes'])])


root = pathlib.Path(sys.argv[1]).resolve()
summary = json.loads((root / 'summary.json').read_text(encoding='utf-8-sig'))
incremental = pathlib.Path(summary.get('build_directory', root / 'build')) / 'debug/incremental'
result = {}
for crate in ('acoui_widgets', 'sigmut'):
    sessions = []
    for unit in incremental.glob(crate + '-*'):
        candidates = [path for path in unit.iterdir() if path.is_dir() and not path.name.endswith('-working')]
        if candidates:
            sessions.append(max(candidates, key=lambda path: path.stat().st_mtime_ns))
    result[crate] = objects([path for session in sessions for path in session.rglob('*.o')])
    result[crate]['sessions'] = [str(path) for path in sessions]
mono_dir, = (root / 'mono').glob('acoui_widgets-test-*')
mono_file, = mono_dir.glob('*.mono_items.json')
mono = json.loads(mono_file.read_text(encoding='utf-8'))
result['widgets_compiler'] = json.loads((mono_dir / 'summary.json').read_text(encoding='utf-8'))
result['widgets_mono'] = dict(
    instantiations=sum(item['instantiation_count'] for item in mono),
    mir_estimate=sum(item['total_estimate'] for item in mono),
    functions=[item for item in mono if item['name'].startswith('sigmut::core::SourceBindings::update')
        or 'sigmut::signal::builder::scan::' in item['name']])
(root / 'analysis.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
print(json.dumps({
    'compiler': result['widgets_compiler'],
    'mono_instantiations': result['widgets_mono']['instantiations'],
    'source_bindings': [item for item in result['acoui_widgets']['families'] if 'SourceBindings::update' in item['name']],
    'widgets_objects': result['acoui_widgets']['total'],
}, indent=2))

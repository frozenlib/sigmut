"""Write a compact comparison from the completed before/final measurement runs."""
import csv
import hashlib
import json
import pathlib
import statistics
import subprocess
import sys

root = pathlib.Path(sys.argv[1]).resolve()
output = pathlib.Path(sys.argv[2])


def read_json(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def run(name):
    path = root / name
    analysis = read_json(path / 'analysis.json')
    rows = list(csv.DictReader((path / 'samples.csv').open(encoding='utf-8-sig', newline='')))
    process_rows = list(csv.DictReader((path / 'processes.csv').open(encoding='utf-8-sig', newline='')))
    return dict(name=name, **analysis['widgets_compiler'],
        build=read_json(path / 'summary.json'),
        sampled_build_peak_private_bytes=max(int(row['own_private_bytes']) for row in rows),
        sampled_link_peak_private_bytes=max((int(row['private_bytes']) for row in process_rows
            if row['own'] == 'True' and row['name'] in ('link.exe', 'lld-link.exe')), default=0))


def condition(names):
    runs = [run(name) for name in names]
    analysis = read_json(root / names[-1] / 'analysis.json')
    widget_families = analysis['acoui_widgets']['families']
    source_update, = [item for item in widget_families if item['name'] == 'sigmut::core::SourceBindings::update']
    return dict(runs=runs,
        compiler_peak_private_bytes_median=statistics.median(item['peak_private_bytes'] for item in runs),
        compiler_elapsed_seconds_median=statistics.median(item['elapsed_seconds'] for item in runs),
        widgets_mono=analysis['widgets_mono'],
        widgets_procedures=analysis['acoui_widgets']['total'],
        source_update_procedures=source_update,
        scan_procedures=[item for item in widget_families if item['name'].startswith('sigmut::signal::builder::scan::')],
        sigmut_code_bytes_in_widgets=sum(item['code_bytes'] for item in widget_families),
        shared_helpers_in_sigmut=[item for item in analysis['sigmut']['families']
            if 'ScanBindings::' in item['name'] or item['name'] in
            ('sigmut::core::Sink::for_update', 'sigmut::core::Sink::sc', 'sigmut::core::Sink::finish_update')])


result = dict(
    baseline='2ba5bf333c0b90d02f2be147d9fd47182f5430ab',
    consumer='b27afca1b5861405fc4edc952a4c8c626c119418',
    rt_local='01bcbd6b6389408db5755af4398d03133a48a88b',
    rustc=subprocess.check_output(['rustc', '-Vv'], text=True).strip(),
    command='cargo test -p acoui-widgets --lib --no-run --locked --offline --jobs 2',
    profile=dict(debug='line-tables-only', opt_level=0, incremental=True, codegen_units=256),
    source_audit=read_json(root / 'source-audit.json'),
    fixture_lock_sha256=hashlib.sha256((root / 'source/Cargo.lock').read_bytes()).hexdigest(),
    final_source_sha256={str(path): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in [pathlib.Path('src/core.rs'), pathlib.Path('src/signal/builder/scan.rs')]},
    before=condition(['before', 'before-2', 'before-3']),
    after=condition(['final-1', 'final-2', 'final-3']),
    limitations=[
        'Windows MSVC, one fixed consumer test target, diagnostic compiler flags enabled.',
        'Private bytes sampled at 20 ms on actual rustc; linker/dependencies are separate.',
        'Build/link sampling is approximately 2-3 seconds, not an exact instantaneous peak.',
        'Widgets incremental artifacts are cleared on repetitions; dependencies are reused.',
        'CodeView lengths describe emitted procedures, not final linked executable size or RAM.',
        'Compiler mono counts and MIR size estimates are not RAM measurements.',
        'No release/runtime benchmark; no full workspace extrapolation.',
    ])
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8', newline='\n')
for label in ('before', 'after'):
    data = result[label]
    print(label, json.dumps(dict(
        private_bytes=[item['peak_private_bytes'] for item in data['runs']],
        median_private_bytes=data['compiler_peak_private_bytes_median'],
        compiler_seconds=[item['elapsed_seconds'] for item in data['runs']],
        mono=data['widgets_mono']['instantiations'], mir=data['widgets_mono']['mir_estimate'],
        source_code=data['source_update_procedures']['code_bytes'],
        scan_code=sum(item['code_bytes'] for item in data['scan_procedures']),
        sigmut_code=data['sigmut_code_bytes_in_widgets'],
        total_code=data['widgets_procedures']['code_bytes'],
    ), indent=2))

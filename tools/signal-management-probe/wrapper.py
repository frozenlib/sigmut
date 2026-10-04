"""Native wrapper.exe forwards rustc arguments here; PROBE_RUN selects output."""
import hashlib
import ctypes
from ctypes import wintypes
import json
import os
import pathlib
import subprocess
import sys
import time

root = pathlib.Path(os.environ['PROBE_RUN'])
args = sys.argv[1:]
manifest = os.environ.get('CARGO_MANIFEST_DIR', '').replace('\\', '/').lower()
source = str(root.parent / 'source').replace('\\', '/').lower()
if '--crate-name' not in args or not manifest.startswith(source + '/'):
    raise SystemExit(subprocess.call(args))
crate = args[args.index('--crate-name') + 1]
kind = 'test' if '--test' in args else 'lib'
key = crate + '-' + kind + '-' + hashlib.sha256('\0'.join(args).encode()).hexdigest()[:10]
out = root / 'mono' / key
out.mkdir(parents=True, exist_ok=True)
args += ['-Zdump-mono-stats=' + str(out), '-Zdump-mono-stats-format=json', '-Ztime-passes']
(out / 'command.json').write_text(json.dumps(args, indent=2), encoding='utf-8')
start = time.time()
class MemoryCounters(ctypes.Structure):
    _fields_ = [('cb', wintypes.DWORD), ('faults', wintypes.DWORD)] + [
        (name, ctypes.c_size_t) for name in ('peak_working', 'working', 'peak_paged',
        'paged', 'peak_nonpaged', 'nonpaged', 'pagefile', 'peak_pagefile', 'private')]

kernel = ctypes.WinDLL('kernel32', use_last_error=True)
psapi = ctypes.WinDLL('psapi', use_last_error=True)
kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
kernel.OpenProcess.restype = wintypes.HANDLE
kernel.CloseHandle.argtypes = [wintypes.HANDLE]
psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(MemoryCounters), wintypes.DWORD]
peak_private = 0
samples = 0
with (out / 'timing.log').open('w', encoding='utf-8') as log:
    process = subprocess.Popen(args, stderr=log)
    handle = kernel.OpenProcess(0x410, False, process.pid)
    if not handle:
        process.wait()
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        while True:
            memory = MemoryCounters()
            memory.cb = ctypes.sizeof(memory)
            if psapi.GetProcessMemoryInfo(handle, ctypes.byref(memory), memory.cb):
                samples += 1
                peak_private = max(peak_private, memory.private)
            if process.poll() is not None:
                break
            time.sleep(0.02)
    finally:
        kernel.CloseHandle(handle)
    code = process.wait()
(out / 'summary.json').write_text(json.dumps(dict(crate=crate, kind=kind, exit_code=code,
    elapsed_seconds=time.time() - start, peak_private_bytes=peak_private, memory_samples=samples), indent=2), encoding='utf-8')
if code:
    sys.stderr.write((out / 'timing.log').read_text(encoding='utf-8', errors='replace'))
raise SystemExit(code)

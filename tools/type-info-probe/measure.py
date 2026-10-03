"""Compare a Git revision with the working tree on Windows/MSVC.

No repository files are replaced. Dependency builds are outside the measured
interval; each downstream rustc invocation writes to a fresh directory.
Only Python's standard library, Git, Cargo and rustc are required.
"""

import argparse
import collections
import ctypes
from ctypes import wintypes
import datetime
import json
import os
from pathlib import Path
import struct
import subprocess
import time
import zipfile


class MemoryCounters(ctypes.Structure):
    _fields_ = [("cb", wintypes.DWORD), ("faults", wintypes.DWORD)] + [
        (name, ctypes.c_size_t)
        for name in (
            "peak_working", "working", "peak_paged", "paged", "peak_nonpaged",
            "nonpaged", "pagefile", "peak_pagefile", "private",
        )
    ]


def compile_measured(command, log):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    psapi = ctypes.WinDLL("psapi", use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    psapi.GetProcessMemoryInfo.argtypes = [
        wintypes.HANDLE, ctypes.POINTER(MemoryCounters), wintypes.DWORD,
    ]
    started = time.perf_counter()
    with log.open("w", encoding="utf-8") as output:
        process = subprocess.Popen(command, stdout=output, stderr=output)
        handle = kernel.OpenProcess(0x410, False, process.pid)
        if not handle:
            process.wait()
            raise ctypes.WinError(ctypes.get_last_error())
        peak_private = 0
        samples = 0
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
        exit_code = process.wait()
    if exit_code:
        raise RuntimeError(f"rustc exited {exit_code}; see {log}")
    return {
        "elapsed_seconds": time.perf_counter() - started,
        "rustc_peak_private_bytes": peak_private,
        "memory_samples": samples,
        "command": command,
    }


def inspect_archive(path):
    data = path.read_bytes()
    assert data[:8] == b"!<arch>\n"
    sections = collections.Counter()
    names = collections.Counter()
    name_bytes = 0
    pos = 8
    while pos + 60 <= len(data):
        size = int(data[pos + 48:pos + 58])
        body = memoryview(data)[pos + 60:pos + 60 + size]
        header_name = data[pos:pos + 16].decode().strip()
        if header_name.startswith("#1/"):
            body = body[int(header_name[3:]):]
        big = len(body) >= 56 and bytes(body[:4]) == b"\0\0\xff\xff"
        normal = len(body) >= 20 and struct.unpack_from("<H", body)[0] == 0x8664
        if big or normal:
            if big:
                count = struct.unpack_from("<I", body, 44)[0]
                start = 56
            else:
                count = struct.unpack_from("<H", body, 2)[0]
                start = 20 + struct.unpack_from("<H", body, 16)[0]
            for index in range(count):
                offset = start + index * 40
                section = bytes(body[offset:offset + 8]).rstrip(b"\0").decode()
                length, raw = struct.unpack_from("<II", body, offset + 16)
                sections[section] += length
                if section != ".debug$T":
                    continue
                cursor = raw + 4
                while cursor + 4 <= raw + length:
                    recsize, leaf = struct.unpack_from("<HH", body, cursor)
                    if recsize < 2:
                        break
                    if leaf == 0x1505 and recsize >= 22:
                        numeric = struct.unpack_from("<H", body, cursor + 20)[0]
                        extra = {
                            0x8000: 1, 0x8001: 2, 0x8002: 2, 0x8003: 4,
                            0x8004: 4, 0x8009: 8, 0x800A: 8,
                        }.get(numeric, 0) if numeric >= 0x8000 else 0
                        name = bytes(body[cursor + 22 + extra:cursor + recsize + 2])
                        name = name.split(b"\0", 1)[0]
                        name_bytes += len(name)
                        names[name.decode(errors="replace")] += 1
                    cursor += recsize + 2
        pos += 60 + size + size % 2
    longest = max(names, key=len, default="")
    return {
        "rlib_bytes": len(data), "sections": dict(sections),
        "structure_distinct_names": len(names),
        "structure_records": sum(names.values()),
        "structure_name_bytes_with_repetitions": name_bytes,
        "longest_structure_name_length": len(longest),
        "longest_structure_name": longest,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", default="92c4d733")
    parser.add_argument("--depths", type=int, nargs="+", default=[0, 3, 6])
    parser.add_argument("--repeat", type=int, default=1)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("This sampler and archive inspector require Windows/MSVC.")
    if min(args.depths) < 0 or args.repeat < 1:
        parser.error("depths must be nonnegative and repeat must be positive")
    repo = Path(__file__).resolve().parents[2]
    # Invoke the compiler itself, not the rustup proxy whose memory is unrelated.
    compiler = subprocess.check_output(["rustup", "which", "rustc"], text=True).strip()
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    output = repo / "target" / "type-info-probe" / stamp
    baseline = output / "baseline-source"
    output.mkdir(parents=True)
    archive = output / "baseline.zip"
    subprocess.run(["git", "archive", args.baseline, "-o", str(archive)], cwd=repo, check=True)
    with zipfile.ZipFile(archive) as zipped:
        zipped.extractall(baseline)
    # Use the exact working-tree dependency lock for both variants.
    lock = repo / "Cargo.lock"
    if lock.exists():
        (baseline / "Cargo.lock").write_bytes(lock.read_bytes())
    result = {
        "baseline": subprocess.check_output(
            ["git", "rev-parse", args.baseline], cwd=repo, text=True,
        ).strip(),
        "working_tree_diff": subprocess.check_output(["git", "diff"], cwd=repo, text=True),
        "rustc": subprocess.check_output([compiler, "-Vv"], text=True),
        "sampling_interval_seconds": 0.02,
        "metric": "downstream rustc PrivateUsage; no dependencies or linker included",
        "cases": [],
    }
    template = Path(__file__).with_name("fixture.rs").read_text(encoding="utf-8")
    (output / "fixture.rs").write_text(template, encoding="utf-8")
    for debug, depths in [("2", args.depths), ("line-tables-only", [max(args.depths)])]:
        for variant, source in [("before", baseline), ("after", repo)]:
            deps = output / f"deps-{variant}-{debug}"
            env = os.environ.copy()
            env.update({
                "CARGO_INCREMENTAL": "0", "CARGO_BUILD_JOBS": "1",
                "CARGO_PROFILE_DEV_DEBUG": debug, "CARGO_PROFILE_DEV_CODEGEN_UNITS": "16",
                "CARGO_PROFILE_DEV_OPT_LEVEL": "0", "CARGO_PROFILE_DEV_LTO": "false",
                "CARGO_ENCODED_RUSTFLAGS": "",
            })
            print(f"Preparing dependencies: {variant}, debug={debug}", flush=True)
            with (output / f"deps-{variant}-{debug}.log").open("w") as log:
                subprocess.run([
                    "cargo", "build", "--offline", "--locked", "--lib",
                    "--manifest-path", str(source / "Cargo.toml"),
                    "--target-dir", str(deps),
                ], cwd=repo, env=env, stdout=log, stderr=log, check=True)
            libraries = list((deps / "debug" / "deps").glob("libsigmut-*.rlib"))
            assert len(libraries) == 1, libraries
            for depth in depths:
                steps = "".join(
                    ".map(|value| value)" if index % 2 == 0 else ".map_value(|value| *value)"
                    for index in range(depth)
                )
                for repetition in range(args.repeat):
                    case = output / f"{variant}-{debug}-depth{depth}-run{repetition}"
                    case.mkdir()
                    fixture = case / "fixture.rs"
                    fixture.write_text(template.replace("/* MAP_STEPS */", steps), encoding="utf-8")
                    command = [
                        compiler, str(fixture), "--crate-name", "type_info_probe",
                        "--crate-type", "rlib", "--edition", "2024",
                        "-C", f"debuginfo={debug}", "-C", "codegen-units=16",
                        "-C", "opt-level=0", "--out-dir", str(case),
                        "--extern", f"sigmut={libraries[0]}",
                        "-L", f"dependency={deps / 'debug' / 'deps'}",
                    ]
                    measured = compile_measured(command, case / "rustc.log")
                    measured.update(inspect_archive(case / "libtype_info_probe.rlib"))
                    measured.update(variant=variant, debug=debug, depth=depth, repetition=repetition)
                    result["cases"].append(measured)
                    (output / "summary.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
                    print(
                        f"{variant} debug={debug} depth={depth}: "
                        f"max name={measured['longest_structure_name_length']}, "
                        f"debug$T={measured['sections'].get('.debug$T', 0) / 2**20:.2f} MiB, "
                        f"private={measured['rustc_peak_private_bytes'] / 2**20:.1f} MiB",
                        flush=True,
                    )
    print(output / "summary.json", flush=True)


if __name__ == "__main__":
    main()

# SourceBindings / ScanNode consumer measurement (Windows MSVC)

This probe builds the fixed acoui widgets test executable against two sigmut
sources. It requires PowerShell 7, Python 3, Git, Cargo, rustc, and cached consumer
dependencies. Use an **independent copy** of the consumer snapshot; never run it
in the original acoui checkout or original measurement directory.

## Fixture

Create `target/signal-management-probe` with this layout:

```text
source/       fixed acoui source snapshot, including Cargo.lock
sigmut/       extracted sigmut revision, later replaced with the changed source
wrapper.exe   compiled from wrapper.rs
wrapper.py    copied from this directory
before/       first baseline run
final-1/      first final implementation run
...
```

For the 2026-10-04 experiment, the baseline is sigmut
`2ba5bf333c0b90d02f2be147d9fd47182f5430ab` and the consumer snapshot is acoui
`b27afca1b5861405fc4edc952a4c8c626c119418`. Extract the baseline with `git archive`
and Python's `tarfile` or `zipfile` (Windows tar may reject Unicode documentation
paths). Copy the complete consumer snapshot to `source`, then append:

```toml
[patch.'https://github.com/frozenlib/sigmut.git']
sigmut = { path = '../sigmut' }
sigmut-macros = { path = '../sigmut/macros' }
```

Run `cargo update -p sigmut --offline --manifest-path <fixture>/source/Cargo.toml`
once. Verify that the lockfile only substitutes these two git packages with path
packages. Preserve that lockfile for every run. Record revisions, toolchain,
source hashes, and profile settings before making implementation changes.

In PowerShell, set `$env:PROBE_PYTHON` to the absolute Python executable path,
copy `wrapper.py` into the fixture, and compile the native launcher:

```powershell
rustc tools/signal-management-probe/wrapper.rs -o target/signal-management-probe/wrapper.exe
pwsh -NoProfile -File tools/signal-management-probe/run.ps1 -RunDirectory D:/projects/sigmut/target/signal-management-probe/before
& $env:PROBE_PYTHON tools/signal-management-probe/functions.py target/signal-management-probe/before
```

The native launcher avoids the Windows batch command line length limit. Its
Python child samples the **actual rustc process**, not the rustup proxy.

Replace the fixture's sigmut source with the changed source only after the
baseline build has finished. Keep the package manifests, consumer source,
lockfile, and features identical. Build the changed source into a separate build
directory using another fresh run directory.

## Repetition

To repeat a condition while reusing built dependencies:

```powershell
pwsh -NoProfile -File tools/signal-management-probe/run.ps1 -RunDirectory D:/projects/sigmut/target/signal-management-probe/before-2 -BuildDirectory D:/projects/sigmut/target/signal-management-probe/before/build -CleanWidgets
& $env:PROBE_PYTHON tools/signal-management-probe/functions.py target/signal-management-probe/before-2
```

`-CleanWidgets` uses `cargo clean -p acoui-widgets` and verifies that its
incremental directory is gone. The widgets compile is therefore cold each time;
dependencies may be warm. The build directory must be inside the fixture.
Run `functions.py` **before the next clean**, since object files are reused by
subsequent runs. Do not compare end-to-end times of cold and warm dependency
builds. Do not change the fixture source while any build is running.

## Conditions and output

- `cargo test -p acoui-widgets --lib --no-run --locked --offline --jobs 2`.
- Consumer profiles supply `debug = "line-tables-only"`; incremental and CGU
  settings are unchanged. This selects widgets' feature closure, not a full
  workspace feature union.
- `RUSTC_BOOTSTRAP=1` is confined to the measuring process. The wrapper adds
  `-Zdump-mono-stats` and `-Ztime-passes` only to consumer workspace crates.
- `mono/*/summary.json`: rustc private bytes peak sampled at 20 ms and elapsed
  time through executable generation. This excludes the separately running
  linker and dependencies.
- `samples.csv` / `processes.csv`: approximately 2–3 second Win32 process samples
  of Cargo/rustc/link, scoped by build output path and parent PID. Unrelated builds
  are recorded separately and excluded from own peaks. CIM sampling may require
  additional host permissions; a monitor failure does not invalidate the
  separately collected rustc samples, but must be reported.
- `analysis.json`: widgets compiler instantiations and MIR estimates; CodeView
  procedure counts, distinct names and code lengths in widgets and sigmut
  incremental objects (only the newest completed session for each crate).
  Common non-generic helpers reside in sigmut's objects,
  which are reported separately rather than silently omitted.
- Mono counts include items beyond function procedures. Neither MIR estimates,
  procedure code lengths, object file sizes, nor RSS phase samples are private
  bytes. Do not describe their percentage changes as RAM savings.

The committed measurement report summarizes the final comparisons; raw build
artifacts and intermediate experiments stay under ignored `target`.

After the three baseline runs (`before`, `before-2`, `before-3`) and the three
final runs (`final-1`, `final-2`, `final-3`), `summarize.py <fixture> <output.json>`
collects the per-run measurements and the last run's object/mono summary. It also
expects `source-audit.json` in the fixture, containing the fixed consumer file
count/hash and the changed/missing/added paths compared with the original
snapshot. In this experiment only Cargo.toml and Cargo.lock differed.

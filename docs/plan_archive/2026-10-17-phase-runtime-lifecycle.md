# Phase identity and Runtime lifecycle

Status: implementation and in-scope verification complete, 2026-10-17 (JST).
Review branch: `codex/phase-runtime-lifecycle`. No integration or publication performed.

Base: `7cbe1c47e145c3233fb3aa8a6b37897bd6571807`. Work is confined to sigmut.
Reference: acoui design recorded in `0df4213dc10515bd7c2f8fc7559b322f5f818a18`.

1. Reproduce lost initial/dirty effects, discard reservations and stream updates.
2. Replace numeric phases with static identities and ordered, fixed lookup tables.
3. Keep reactions and lifetime maintenance in TLS, counting runnable reactions only.
4. Add irreversible Closing; cancel Actions outside internal borrows with context intact.
5. Audit wake, ownership and persistent-to-Action bridges; document migration contracts.
6. Verify regression tests, formatting, Clippy, tests and docs; archive this plan and commit.

No acoui/rt-local edits, merge, rebase, push, PR or publication are authorized.

New API tests cannot run against the old signatures; existing API lifecycle regressions
are run first. Phase and Closing tests will then exercise the replacement API.

## Implemented

- Static, lazily allocated PhaseIDs; shared `DEFAULT` references; fixed O(1)
  lookup tables and independently configured dispatch order.
- Permanent phase queues, runnable Reaction count, and TLS-owned discards and
  maintenance. Queue snapshots contain counts, preserving unexecuted reservations
  even when an earlier callback unwinds.
- Irreversible Closing, out-of-borrow capture/Future cancellation, preserved async
  cancellation context, inactive old wake tokens, and cleared driver wakers.
- No user Future polling through direct Signal reads during Closing; persistent
  futures resume in a Running runtime. Future destruction is outside node data borrows.
- Shared readiness for waiting and flush, including Actions created by maintenance.
- Consumer-owned bridge recovery tested with revision acknowledgments and weak owners.
  There is no built-in production bridge to modify in sigmut.
- Removed the obsolete numeric bucket/map implementation. No dependencies changed.

The public contract and acoui migration work are recorded in
[`docs/runtime-lifecycle.md`](../runtime-lifecycle.md).

## Regression-first evidence

The initial compatible tests failed before the implementation: initial effect,
Dirty effect, discard reservation (all lost on Runtime Drop), and explicit Action
registration without a Runtime. These four passed after the ownership change.
Stream / async wake / real discard flag / A → B → A tests exercise other consumers
of the same reservation ownership repair.

Further minimal failing tests were run before each corresponding fix:

- oneshot Waker wake / replacement / cancellation under the Mutex: three failures;
- cancelled Action wake incorrectly waking the next driver: one failure;
- completed effect_async Future dropped under its node borrow: one failure;
- direct reads polling future / stream / async Signals in Closing: three failures;
- flush returning before an Action created by carried notification: one failure;
- callback unwind destroying later Reaction reservations / pending Action captures:
  two failures.

Phase signatures and explicit Closing are new APIs; their final contract tests
cannot compile against the earlier signatures. Identity, overflow, configured
order, default-only behavior, duplicates, dormant readiness, owner release,
Closing reentry, context isolation, cross-thread wake, bridge ack/recovery and
absence of automatic external-effect replay were tested after introducing them.

## Final verification

All final runs started at HEAD `7cbe1c47e145c3233fb3aa8a6b37897bd6571807`
with this task's uncommitted changes. Sources were held fixed throughout each run.
The SHA256 of the path/content manifest covering every Rust file in `src`, `tests`,
`macros`, plus root `Cargo.toml` and `Cargo.lock`, was checked before and after:
`49b2c7e107d5f859388434f4d4e5043f763579a66baabc091c09fccd271bb25a`.
Only this report was completed after verification.

Toolchain: stable rustc 1.99.0; Miri nightly 2026-10-01.

| Command / configuration | Actual result |
| --- | --- |
| `cargo fmt --all --check` | Passed |
| `cargo test --workspace --all-features` | 375 ordinary tests + 12 doc tests passed; existing ignored trybuild test remains ignored |
| `RUSTFLAGS="-D warnings" cargo clippy --all-features --tests --lib -- -W clippy::all` | Passed |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p sigmut` | Passed |
| Direct minimal versions, then the same workspace test | 375 ordinary tests + 12 doc tests passed |
| CI Miri command without extra skip | Failed in the pre-existing test described below |
| CI Miri command with that one test skipped at invocation | 368 tests passed; CI's six should-panic exclusions and existing ignored trybuild test retained |

Minimal versions were resolved online with `cargo +nightly update -Z direct-minimal-versions`
in a byte-matched source copy under `target/verification/minimal`, then tested there.
The original lockfile was preserved. Initial offline resolution was insufficient
because it selected cached versions, so it was not counted as minimal-version verification.

Miri setup needed a temporary directory outside the workspace and a dedicated
`MIRI_SYSROOT=target/miri-sysroot`; the sandbox's default temporary path contains
braces interpreted as Cargo build-dir variables. Setup and execution subsequently worked.
Local logs and source manifests are in the ignored `target/verification` directory.

### Registry-version Miri limitation (resolved by the follow-up below)

`dropping_subscription_wakes_waiting_runtime_for_discard` fails Stacked Borrows
at `drop(lend)` with `trying to retag ... Unique permission ... tag does not exist`.
The trace runs through the `rt_local::runtime::core::test` macro and
`rt-local-core 0.1.3`'s `RawRunnable::run`. Adopted registry sources were inspected
using the unchanged Cargo.lock (`rt-local 0.1.4`, `rt-local-core 0.1.3`,
`rt-local-macros 0.1.4`).

The exact same test and failure were reproduced in an independent `git archive HEAD`
copy of the unmodified base under `target/verification/baseline`. This establishes
that the failure predates this change; it does not establish a complete diagnosis
or a fix for that dependency path. Neither the test nor rt-local was altered or
disabled in source. The full Miri run is therefore **not** claimed as passing.

Final Miri command:

```text
cargo +nightly miri test --lib --bins --tests -- -Z unstable-options --exclude-should-panic --skip dropping_subscription_wakes_waiting_runtime_for_discard
```

At that verification point, the existing Miri issue, acoui integration, and external
Tokio/UI resource lifetime integration remained unverified. acoui must implement its own bridge revisions,
phase migration and drain/Closing boundary before adopting this contract; see the guide.

## Follow-up: GitHub rt-local update, 2026-10-17

The user requested testing the latest GitHub rt-local against the Miri failure.
`git ls-remote --symref` confirmed that the repository has no `main` branch and
its default branch is `master`, at `cd039904282f0376e811c805d4fdaa9cbc6b2219`.
The dev-dependency now tracks `branch = "master"` without a `rev` pin; Cargo.lock
records that commit for rt-local, rt-local-core and rt-local-macros. Other locked
dependencies were preserved.

The adopted `core/src/base_impl.rs` changes `RawRunnable::run` to project the
pinned mutable reference before inspecting cancellation, borrowing only `task`.
The previous field access through `Pin` borrowed the whole runnable, including
references stored in the suspended future. The source change and the passing
reproduction are consistent with resolving the reported invalidation.
No rt-local source was edited in this task.

The latest test entry point is `runtime::blocking`, so the two sigmut test uses
of `runtime::core` were migrated. Assertions, test contents and enabled cases
were unchanged. The original failing Miri test now passes on its own, and the
full CI Miri invocation passes **without the previous extra skip**:

```text
cargo +nightly miri test --locked --lib --bins --tests -- -Z unstable-options --exclude-should-panic
```

Verification started at HEAD `a4b1372ab808fb60fbe11811bf49a459ed8af3cc`
with only the dependency, lockfile and two test API migrations pending.
The source manifest was unchanged before/after all runs:
`1591b6285b3e6775101ca95896695f523bf6ac36222a9b355e1e723d80e4b582`.

| Check | Actual result |
| --- | --- |
| Formerly failing Miri reproduction | Passed |
| Full CI Miri command above | 369 passed; only CI's six should-panic exclusions and the existing ignored trybuild test retained |
| `cargo test --locked --workspace --all-features` | 375 ordinary tests + 12 doc tests passed |
| Direct minimal versions in the isolated copy, then workspace tests | 375 ordinary tests + 12 doc tests passed, using the same rt-local commit |
| `cargo fmt --all --check` | Passed |
| Clippy with `RUSTFLAGS="-D warnings"`, all features, tests and library | Passed |

Logs are `target/verification/*rt-local-latest*`. The earlier Miri limitation is
resolved with this Git dependency; acoui adoption and external-resource integration
remain outside this task. No merge, push, PR, publication or rt-local source edit
was performed.

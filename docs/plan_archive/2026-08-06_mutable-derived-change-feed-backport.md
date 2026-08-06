# mutable derived ChangeFeed バックポート計画

## 目的

acoui で実 consumer とともに検証した mutable derived ChangeFeed を sigmut へ汎用 API として移し、既存の `ChangeFeedState::new`、`ChangeFeedSignal::from_scan`、および `db156d9` の generic `ScanNode` MaybeDirty 解決を維持する。

## 実装

1. `ChangeFeedState::from_scan(initial, scan)` を `ChangeFeedSignal::from_scan` と対称な closure contract で追加する。
2. derived state の source invalidation を downstream へ MaybeDirty として伝播し、context 付き access、dependency check、mutable borrow の前に source を解決する。
3. scan が change を記録した場合だけ downstream を Dirty、記録しなかった場合は clean として解決する。
4. source invalidation と scheduled local edit に別 slot を使い、確定した local Dirty が equal source reconciliation に吸収されないようにする。
5. scan mutation と direct mutation を既存 `ChangeFeedStorage` の単一 ordered history に記録し、初回 reader delta と contextless borrow の既存契約を維持する。
6. public API と module documentation に source resolution、direct edit、contextless access の契約を反映する。

## テスト

- 最新 source の初回 materialization 後も reader delta が `Initial` である。
- reader の `read` と `peek` が access 前に source を解決し、`peek` は cursor を進めない。
- `borrow_mut` と `borrow_mut_loose` が source を先に解決し、scan/direct change の順序を保つ。
- equal source notification が downstream bindings を clean に解決する。
- MaybeDirty source が clean に解決された場合は scan を再実行せず、downstream bindings も clean に解決する。
- loose edit の Dirty が equal source update に吸収されない。
- `try_borrow_contextless` が source を評価せず、最後に materialize 済みの値を返す。
- 既存 ChangeFeed baseline tests と generic `ScanNode` regression tests を含む全 target を実行する。

## 検証

1. 対象 nextest filter
2. `cargo fmt`
3. `cargo clippy --fix --allow-dirty --allow-staged --all-targets`
4. `cargo nextest run --all-targets`
5. `cargo test --doc`
6. `cargo doc --no-deps --workspace --lib`
7. `cargo llvm-cov` の JSON/text report と追加コードの未カバー箇所確認
8. `git diff --check`

## 対象外

- acoui 固有の provenance、dead-code suppression、module visibility
- acoui にコピーした private support
- 新規 unsafe code または macro
- master への merge、remote push

## 実装結果

- `ChangeFeedState::from_scan` と optional scan data を追加し、既存 `ChangeFeedStorage`、`Changes`、`RefCountOps` を再利用した。
- contextual borrow、reader read/peek、dependency check、`borrow_mut`、`borrow_mut_loose` を同じ source resolution へ収束させた。
- source invalidation と scheduled local edit を `Slot(0)` / `Slot(1)` に分離し、scan と direct edit を同じ ordered history に保持した。
- public module docs と constructor docs に mutable derived state、初回 `Initial` delta、direct mutation の契約を反映した。
- acoui 固有の provenance、suppression、private support は移植せず、新規 unsafe code と macro も追加しなかった。

## 検証結果

- `cargo fmt --check`: 成功
- `cargo clippy --fix --allow-dirty --allow-staged --all-targets`: 成功
- `cargo clippy --all-features --tests --lib -- -W clippy::all`: 成功
- `cargo nextest run --all-targets`: 263 passed、1 skipped
- `cargo test --doc --workspace`: 10 passed
- `cargo doc --no-deps --workspace --lib`: 成功
- `cargo llvm-cov`: ChangeFeed 本体 353/368 lines、95.92%。追加した MaybeDirty-clean 分岐を含む契約経路をカバーした。
- `git diff --check`: 成功

Windows の各 cargo command は既存 proc-macro DLL import library 生成に関する `linker_messages` warning を表示したが、Clippy diagnostic と test failure はなかった。

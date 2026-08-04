# `ChangeFeedState` の context なし試行借用 実装計画

## 目的

外部 crate が `ChangeFeedState` を Signal / State 実装基盤として利用し、serialization や diagnostic formatting のように `SignalContext` を受け取れない処理から、実体化済みの現在値を安全に試行借用できるようにする。

## 確定仕様

- `ChangeFeedState::try_borrow_contextless(&self) -> Result<ChangeFeedRef<'_, M>, std::cell::BorrowError>` を public API として追加し、`borrow_untracked` を置き換える。
- 通常の読み取りには `borrow(sc)` を使用し、非追跡のリアクティブ読み取りには追跡を無効化した `SignalContext` を通常の `borrow` / `read` へ渡す。
- context なし試行借用は、実体化済みの `ChangeFeedState` の現在値だけを借用する。依存登録、派生状態評価、reader cursor 操作は行わない。
- 可変借用との競合は panic せず `BorrowError` を返す。
- Storage の fallible helper は crate 内部に限定する。
- `StateVec` の serialization は `BorrowError` を serializer error へ変換する。
- `ReactionContext` を受け取る `borrow_untracked`、`ChangeFeedSignal` / `ChangeFeedReader` の contextless API は追加しない。

## 実装手順

1. `ChangeFeedStorage` に、圧縮やcursor操作を行わず `RefCell::try_borrow` で現在の履歴を借用する内部helperを追加する。
2. `ChangeFeedState::borrow_untracked` を `try_borrow_contextless` に置き換え、通常利用と低レベル用途の違いをdoc commentに記載する。
3. `StateVec` と外部 crate 相当のSerialize実装を新APIへ移行し、借用競合をserde errorへ変換する。
4. 正常借用、借用競合、Serialize成功、Serialize競合をテストする。
5. rustfmt、Clippy、対象テスト、全targetテスト、doc test、rustdoc、coverageを実行する。
6. 実装結果を追記し、完了後にこの計画書を `docs/plan_archive` へ移動する。

## テスト計画

- context なし試行借用が現在値を返し、readerの初回cursorを変えない。
- 可変編集ガードの保持中は context なし試行借用が `Err` を返す。
- 外部 crate 相当のfacadeが公開APIを使ってSerializeを実装できる。
- `StateVec` と外部facadeのSerializeが通常時に成功する。
- 可変編集ガードの保持中はSerializeがpanicせずserde errorを返す。

## 実装結果

- `ChangeFeedStorage` に、圧縮・依存登録・派生評価・reader cursor操作を行わず、現在の履歴を `RefCell::try_borrow` する内部helperを追加した。
- `ChangeFeedState::borrow_untracked` を public な `try_borrow_contextless` に置き換え、通常読み取りと非追跡読み取りには `SignalContext` を使うこと、contextなしAPIはserialization / diagnostic formatting向けであることをrustdocに記載した。
- `StateVec` と外部 crate 相当のfacadeのSerialize実装を新APIへ移行し、`BorrowError` をserde errorへ変換した。
- 正常借用、reader cursor維持、可変借用競合、外部facadeと `StateVec` のSerialize成功・競合をテストした。
- `cargo fmt --check`、`cargo clippy --fix --allow-dirty --allow-staged --all-targets`、`cargo check --workspace --all-targets`、対象テスト、`cargo nextest run --all-targets`（247 passed、1 skipped）、`cargo test --doc`（10 passed）、`cargo doc --no-deps --workspace`、`cargo llvm-cov --workspace --all-targets --summary-only`を実行し、すべて成功した。coverageは全体89.57% lines、`change_feed.rs` 94.60% linesだった。

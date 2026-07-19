# `SignalVecReader::peek` 実装計画

- 作成日: 2026-07-20
- 状態: 実装完了

## 目的

`SignalVecReader::read` の cursor 更新契約を維持したまま、同じ cursor を基準に snapshot と changes を非破壊で参照する `SignalVecReader::peek` を追加する。

## 実装方針

1. `SignalVecNode` に非破壊読み取り経路を追加し、`StateVec`、`SignalVec::from_scan`、immutable `Vec` のいずれでも source の dependency 登録後に reader の現在 age を基準とする `Items` を返す。
2. `Items` の構築を「指定 age から読む処理」と「reader を末尾へ進める処理」に分離し、`peek` が age や履歴保持用参照カウントを変更しない構造にする。
3. reader が一度 `read` した後の age は source の履歴保持対象として維持し、`peek` の反復やその間の source 更新でも changes が失われないことを確認する。`from_scan` も同じ履歴保持規則を通す。
4. 公開 `peek` の rustdoc に、cursor を進めないこと、`read` と同様に dependency を登録すること、返された参照の生存中は source の retained state を変更できないことを記載する。呼び出し側に追加の `Element` 状態制約が必要かは、Rust の borrow による保証範囲を確認して決定する。

## テスト計画

- `StateVec`: 初回 `peek` が全要素の Insert を返し、反復 `peek` と直後の `read` が同じ changes を返す。
- `StateVec`: `peek` 後に変更を追加すると、次の `peek` と `read` が前回 `read` 以降の全変更を返す。
- `SignalVec::from_scan`: dependency 更新を反映しつつ、`peek` が cursor を進めず履歴を保持する。
- immutable slice / `Vec`: 初回 `peek` と直後の `read` が同じ全要素 Insert を返し、`read` 後の `peek` は changes を返さない。

## 検証手順

1. 追加した対象テストを `cargo nextest run -p sigmut <filter>` で実行する。
2. `cargo fmt` と `cargo clippy --fix --allow-dirty --allow-staged --all-targets` を実行する。
3. `cargo nextest run --all-targets` と `cargo test --doc` を実行する。
4. `cargo llvm-cov` で変更経路の coverage を確認する。
5. `cargo doc --no-deps --workspace --lib` を実行する。

## 対象外

- 明示的な ack API。
- `SignalVecReader` の `Clone`。
- cursor 値または rewind API の公開。

## 完了条件

- `peek` が reader の cursor を進めず、全 source 種別で同じ契約を満たす。
- `read` の既存契約と API が維持される。
- 対象テスト、全テスト、doc test、Clippy、rustdoc が成功する。
- 完了した本計画書が `docs/plan_archive` に移され、変更がコミットされる。

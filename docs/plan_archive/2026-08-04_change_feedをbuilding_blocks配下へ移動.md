# `change_feed` を `building_blocks` 配下へ移動

## 目的

特定用途向けの `Signal` / `State` を構築する公開部品を `building_blocks` 名前空間に集約する。その最初の部品として、現在crate直下にある `change_feed` を `sigmut::building_blocks::change_feed` へ移動する。

## 方針

- crate直下に `building_blocks` 公開モジュールを追加し、その配下で `change_feed` を公開する。
- `change_feed` の実装と単体テストをRustのモジュール構成に対応するディレクトリへ移す。
- crate内の利用箇所、公開APIの統合テスト、doc testのimportを新しい公開パスへ更新する。
- 旧 `sigmut::change_feed` は互換エイリアスを設けず削除する。

## 検証

- `cargo fmt`
- `cargo clippy --fix --allow-dirty --allow-staged --all-targets`
- `cargo nextest run -p sigmut external_state_facade_uses_public_change_feed_api`
- `cargo nextest run --all-targets`
- `cargo test --doc`
- `cargo doc --no-deps --workspace --lib`
- `cargo doc --no-deps --workspace`

## 実装結果

- `building_blocks` 公開モジュールを追加し、`change_feed` の実装と単体テストをその配下へ移動した。
- crate内のコレクション実装、外部API統合テスト、doc testを `sigmut::building_blocks::change_feed` に更新した。
- 旧 `sigmut::change_feed` は削除した。
- フォーマット、Clippy、全テスト、doc test、Rustdoc生成が成功した。

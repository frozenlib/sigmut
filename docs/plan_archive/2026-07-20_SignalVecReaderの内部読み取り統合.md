# `SignalVecReader` の内部読み取り統合 計画

- 作成日: 2026-07-20
- 状態: 実装完了

## 目的

`SignalVecNode` の重複した `read` / `peek` 実装を、snapshotを取得する `peek` とreaderのcursorを進める `advance_reader` に分離する。公開 `SignalVecReader::read` / `peek` の契約は変更しない。

## 実装方針

1. `SignalVecNode::read` を削除し、`#[must_use] fn advance_reader(age: Option<usize>) -> usize` を追加する。
2. `SignalVecReader::read` は現在のageで `peek` した直後に `advance_reader` の戻り値を保存する。
3. `StateVec` と `SignalVec::from_scan` の履歴参照カウント移動と末尾age取得を共通化する。
4. immutable slice / `Vec` はcursorを `0` へ進め、履歴参照カウントを持たない既存の意味を維持する。

## 検証

- `StateVec`、`from_scan`、immutable sourceの既存 `read` / `peek` 契約テストを実行する。
- format、Clippy、全体テスト、doc test、rustdocを実行する。

## 完了条件

- `SignalVecNode` の読み取り実装が `peek` に一本化される。
- `advance_reader` の戻り値にメッセージなしの `#[must_use]` が付く。
- 既存の公開契約と全テストが維持される。
- 完了後に本計画書を `docs/plan_archive` へ移し、変更をコミットする。

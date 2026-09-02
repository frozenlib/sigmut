# `Signal` と `SignalVec` の共有定数表現 計画

## 目的

`Signal<T>` が不変値の `Rc<T>` / `Arc<T>` を型消去せず保持できるようにし、既存の共有所有権から追加割り当てなしで変換できるようにする。

`SignalVec<T>` は既存の `Rc<Vec<T>>` と static slice の表現を維持しつつ、`Rc<[T]>` / `Arc<[T]>` を直接保持できるようにする。変更履歴を持つ表現は、実装内容を表す `ChangeFeed` というvariant名にする。

## 変更内容

1. `RawSignal<T>` に `Rc(Rc<T>)` と `Arc(Arc<T>)` を追加する。
2. `Signal::from_value` は `Rc<T>` 表現を使用する。
3. `Rc<T>`、`&Rc<T>`、`Arc<T>`、`&Arc<T>` から `Signal<T>` への `From` を実装する。
4. `Signal::from_static_ref` と `StaticRef` 表現は維持する。
5. `RawSignalVec<T>` と `RawSignalVecReader<T>` を `ChangeFeed`、`RcVec`、`RcSlice`、`ArcSlice`、`StaticSlice` の構成にする。
6. `Vec<T>` と `Rc<Vec<T>>` は `RcVec` 表現を使用する。
7. `[T; N]`、`Rc<[T]>`、`&Rc<[T]>`、`Arc<[T]>`、`&Arc<[T]>` から `SignalVec<T>` への `From` を実装する。
8. 既存の static slice / static array reference の変換は維持する。

## 検証

- `Signal` の各共有所有権変換について、値、定数判定、ポインタ同一性、参照カウントを確認する。
- `Signal::from_value` のcloneと独立生成に対するポインタ同一性を維持する。
- `SignalVec` の各不変表現について、borrow、初回reader delta、2回目以降の空deltaを確認する。
- `[T; N]` と `Vec<T>` の変換が `T: Clone` を要求しないことを確認する。
- format、Clippy、全テスト、doc test、rustdocを実行する。

## 完了

- `RawSignal` に `Rc` / `Arc` 表現と対応する所有・参照変換を追加した。
- `Signal::from_value` を `Rc` 表現へ移行し、`from_static_ref` は維持した。
- `RawSignalVec` とreaderを `ChangeFeed` / `RcVec` / `RcSlice` / `ArcSlice` / `StaticSlice` に更新した。
- `Vec<T>` と `Rc<Vec<T>>` は `RcVec`、所有配列は `RcSlice` を使用し、共有sliceの所有・参照変換を追加した。
- format、Clippy、全テスト、doc test、rustdoc、coverageを実行し、すべて成功した。

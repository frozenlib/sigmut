# `SignalVec` の `ArcVec` 表現 計画

## 目的

既存の `Arc<Vec<T>>` を要素のcloneや別の共有領域への変換なしで `SignalVec<T>` の不変sourceとして利用できるようにする。

## 変更内容

1. `RawSignalVec<T>` に `ArcVec(Arc<Vec<T>>)` を追加する。
2. `RawSignalVecReader<T>` に対応する `ArcVec` 表現を追加し、readerが所有権を保持する。
3. `Arc<Vec<T>>` と `&Arc<Vec<T>>` から `SignalVec<T>` への `From` を実装する。
4. 共有所有権APIの対称性のため、`&Rc<Vec<T>>` からの `From` も実装する。
5. `Vec<T>` の変換は既存どおり `RcVec` を使用する。

## 検証

- 所有値と参照の各変換で現在値を参照できることを確認する。
- 参照からの変換が `Rc` / `Arc` の所有権をcloneすることを確認する。
- `ArcVec` readerの初回delta、peek、clone後の独立cursorを確認する。
- `T: Clone` を要求しないことを確認する。
- format、Clippy、全テスト、doc test、rustdoc、coverageを実行する。

## 完了

- `RawSignalVec` / `RawSignalVecReader` に `ArcVec` を追加した。
- `Arc<Vec<T>>`、`&Arc<Vec<T>>`、`&Rc<Vec<T>>` の変換を追加した。
- reader semantics、参照カウント、非`Clone`要素をテストした。
- format、Clippy、全テスト、doc test、rustdoc、coverageが成功した。

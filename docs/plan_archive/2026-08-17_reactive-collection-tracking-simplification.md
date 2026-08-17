# リアクティブコレクション追跡単位の単純化計画

## 目的

コレクションの性質に応じて依存追跡単位と編集 API を分け、不要なキー単位通知と編集ガードの処理を削除する。

## API 方針

- Vec、HashMap、HashSet はコレクション全体を一つのリアクティブな状態として扱う。
- HashMap / HashSet の Signal と State から、借用を介さない `get` / `contains_key` / `contains` を削除する。
- HashMap / HashSet の `*Ref` / `*RefMut` には、非リアクティブな要素参照メソッドを維持する。
- HashMap / HashSet の編集は `borrow_mut` / `borrow_mut_loose` を使用する。
- SlabMap は発行済みキー単位の追跡を維持し、State の `insert` / `remove` で変更する。
- StateSlabMap から `borrow_mut` / `borrow_mut_loose` を削除する。
- SignalSlabMap の scan には、変更記録付きの `SlabMapRefMut` を引き続き渡す。
- SlabMap の不正キーに対する `item` / `remove` のパニック契約を維持する。

## 実装手順

1. HashMap のキー単位 binding、slot 変換、変更 slot 収集、個別通知を削除する。
2. HashSet の要素単位リアクティブ API と通知テストを削除し、HashMap の全体追跡へ集約する。
3. SlabMap の State 編集を直接 `insert` / `remove` に戻し、編集ガード用の通知状態を scan 専用に単純化する。
4. crate 内の利用箇所、テスト、rustdoc を新しい契約へ移行する。
5. format、Clippy、全 target テスト、doc test、rustdoc、coverage を実行する。

## 完了条件

- HashMap / HashSet にキー・値単位の reactive binding 実装が残っていない。
- HashMap / HashSet の全リアクティブ読み取りが `borrow` を経由する。
- StateSlabMap が `insert` / `remove` を提供し、公開 mutable borrow を提供しない。
- SlabMap の全体通知と変更キー通知が一度の直接変更で正しく行われる。
- 全検証が成功し、本計画書が `docs/plan_archive` に移動される。

## 実装結果

- HashMap のキーから通知 slot への変換、slot binding、変更 slot の蓄積、個別通知を削除し、標準の `ChangeFeedSignal` / `ChangeFeedState` に集約した。
- HashSet の要素単位リアクティブ API を削除し、HashMap と同じコレクション全体の依存追跡へ統一した。
- Signal / State の `get` / `contains_key` / `contains` を削除し、借用後の `*Ref` / `*RefMut` にのみ非リアクティブな要素参照を残した。
- StateSlabMap の `borrow_mut` / `borrow_mut_loose` を削除し、`insert` / `remove` が全体と対象キーを直接通知する構成へ戻した。
- SlabMap の scan 用 `SlabMapRefMut` は変更履歴から対象キーを得るようにし、重複したキー蓄積と Drop 時通知処理を削除した。
- 使用例、テスト、rustdoc を新しい契約へ更新した。

## 検証結果

- `cargo fmt --all -- --check`: 成功
- `cargo clippy --fix --allow-dirty --allow-staged --all-targets`: 成功
- `cargo nextest run --all-targets`: 294 件成功、1 件スキップ
- `cargo test --doc`: 通常 7 件、compile-fail 3 件成功
- `cargo doc --no-deps --workspace --lib`: 成功
- `cargo llvm-cov --all-targets --summary-only`: line coverage 91.62%
- `git diff --check`: 成功

Windows 環境由来の linker import-library メッセージ以外に、新しい警告は発生していない。

# リアクティブコレクション API 統一計画

## 目的

`collections` 配下のリアクティブコレクションについて、全体借用、要素単位読み取り、編集、State から Signal への変換の語彙と構成を統一する。

## 公開 API 方針

- コレクション全体の追跡付き読み取りは `borrow` とする。
- 借用ガードはコレクションごとの `*Ref`、変更記録付き編集ガードは `*RefMut` とする。
- State から Signal への変換は `to_signal` とする。
- State のコレクション変更は `borrow_mut` / `borrow_mut_loose` が返す `*RefMut` に集約する。
- HashMap の要素読み取りは、欠損が通常状態であるため `get` / `contains_key` とする。
- HashSet の要素読み取りは `contains` とする。
- SlabMap のキーは有効な要素ハンドルであることを契約とし、要素読み取りはパニックする `item` とする。公開 `get` / `contains_key` は提供しない。
- Vec は添字単位の依存追跡を提供しないため、要素読み取りは `borrow(sc).get(index)` とする。
- reader の `read` / `peek` と借用ガードの `delta` は維持する。

## 型名

- `vec::Items` / `ItemsMut` を `VecRef` / `VecRefMut` に変更する。
- `hash_map::Items` / `ItemsMut` を `HashMapRef` / `HashMapRefMut` に変更する。
- `hash_set::Items` / `ItemsMut` を `HashSetRef` / `HashSetRefMut` に変更する。
- `slab_map::Items` / `ItemsMut` を `SlabMapRef` / `SlabMapRefMut` に変更する。

## SlabMap 内部構造

現在値を保持する `SlabMapModel` と変更記録付きの `SlabMapRefMut` を分離する。`SlabMapRefMut` は `ChangeFeedRefMut` を保持し、各操作で現在値の更新と change の記録を同じ経路で行う。

State の `borrow_mut`、`borrow_mut_loose` と Signal の `from_scan` は同じ編集ガードを使用する。編集ガードは変更されたキーを保持し、State 編集終了時にはコレクション全体と該当キーの依存先を正しく通知する。

## 実装手順

1. Vec、HashMap、HashSet の型名とメソッド名を変更する。
2. HashMap / HashSet の直接 State 変更メソッドを削除し、編集ガードへ集約する。
3. HashMap / HashSet に `borrow_mut_loose` を追加し、キー単位通知を遅延通知でも維持する。
4. SlabMap のモデルと編集ガードを分離する。
5. SlabMap に `borrow`、`borrow_mut`、`borrow_mut_loose`、`to_signal` を実装する。
6. SlabMap の公開 `get` を削除し、`item` と `Index` のパニック契約をテストする。
7. crate 内の利用箇所、テスト、rustdoc を新 API へ移行する。
8. format、Clippy、全 target テスト、doc test、rustdoc、coverage を実行する。

## 完了条件

- 全コレクションが `borrow` / `*Ref` と `borrow_mut` / `*RefMut` の対称な API を持つ。
- 全 State コレクションが `to_signal`、`borrow_mut_loose` を持つ。
- SlabMap の不正キー読み取りと削除が一貫してパニックする。
- HashMap / HashSet / SlabMap の要素単位通知が通常編集と loose 編集の双方で正しい。
- 全検証が成功し、本計画書が `docs/plan_archive` に移動される。

## 実装結果

- Vec、HashMap、HashSet、SlabMap の全体読み取りを `borrow`、借用型をコレクション別の `*Ref` / `*RefMut` に統一した。
- 全 State コレクションの Signal 変換を `to_signal` に統一し、変更操作を `borrow_mut` / `borrow_mut_loose` の編集ガードに集約した。
- HashMap / HashSet / SlabMap の loose 編集でも、コレクション全体に加えて変更された要素だけを遅延通知するようにした。
- SlabMap の保持モデルと変更記録付き編集ガードを分離し、State と scan が同じ変更経路を使用するようにした。
- SlabMap は公開 `get` / `contains_key` を持たず、`item` / `Index` と不正キー削除のパニック契約を rustdoc とテストで固定した。
- crate 内の利用箇所と example を新 API に移行した。

## 検証結果

- `cargo fmt --all`
- `cargo clippy --fix --allow-dirty --allow-staged --all-targets`
- `cargo nextest run --all-targets`: 293 passed、1 skipped
- `cargo test --doc`: 通常 doctest 7 件、compile-fail doctest 3 件に成功
- `cargo doc --no-deps --workspace --lib`
- `cargo llvm-cov --all-targets --summary-only`: line coverage 90.30%

Windows MSVC linker が proc-macro DLL の import library 作成メッセージを warning として報告する既存の出力を除き、検証は成功した。

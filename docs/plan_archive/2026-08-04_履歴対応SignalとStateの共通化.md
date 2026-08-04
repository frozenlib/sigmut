# 履歴対応 Signal / State の共通化設計

> 後続の名称変更により、公開モジュールは`change_feed`、公開型は`ChangeFeed*`、借用ガードは`ChangeFeedRef` / `ChangeFeedRefMut`、初回／差分を表すenumは`ChangeFeedDelta`となった。本文では設計・実装時点の名称を保持する。

## 目的

変化する `SignalVec` に固有として実装されている次の責務を、値の種類に依存しない履歴対応 Signal / State の共通基盤に分離する。

- 現在値と変更履歴の保有
- State の編集ガードと scan の更新関数による履歴付き更新
- reader ごとの独立した cursor
- cursor 以降の履歴保持と、どの reader からも保持されなくなった履歴の解放
- 履歴が増えた場合だけの reactive notification / dirty 更新

初期実装では共通基盤を crate-private にし、`SignalVec` / `StateVec` をその基盤の最初の利用者にする。一般利用者向け API としての公開は、別の履歴モデルで再利用して API を検証した後に別件で行う。

## 現行実装の分析

`src/collections/vec.rs` では、共通化したい責務が次の複数の型に分散している。

| 責務 | 現行の実装 |
| --- | --- |
| 現在値と Vec 固有の参照先保持 | `ItemsData<T>` |
| 履歴本体と age | `utils::Changes<ChangeData>` |
| snapshot が `Ref` を保持中でも行える reader 操作 | `utils::RefCountOps` |
| reader の clone / advance / drop | `SignalVecReader` と `SignalVecNode` の 3 メソッド |
| State の更新と通知 | `RawStateVec` と `ItemsMut::drop` |
| scan の更新と dirty 判定 | `Scan` / `ScanData` |
| 不要履歴が参照する旧値の解放 | `ItemsData::clean_changes` |

`Changes` と `RefCountOps` 自体は汎用的だが、両者を必ず同期させる不変条件は利用側に露出している。そのため State と scan が同じ reader 処理、編集開始処理、履歴解放処理を個別に持っている。

`SignalSlabMap` にも同様の重複があるため共通化の候補になるが、初期実装では `SignalVec` の移行にスコープを限定する。

## 設計方針

### 1. 変更の意味はモデル側に残す

`VecChange` の insert / remove / move などは Vec 固有である。共通基盤は変更内容を解釈せず、「現在値に対応するモデル」と「モデル固有の変更データ」の組を扱う。

```rust
pub(crate) trait HistoryModel: 'static {
    type Change: 'static;

    // 履歴から取り除かれる Change が保持していた
    // モデル固有リソースを解放する。
    fn release_change(&mut self, change: Self::Change);
}
```

`SignalVec` 用には、現行の `ItemsData<T>` から履歴コンテナを除いた `VecModel<T>` を作る。`VecModel<T>` は `items` と `values` を持ち、`Remove` / `Set` の旧値が履歴から外れたときに `values` から対応値を解放する。

### 2. 現在値、履歴、reader 操作を `HistoryCell` に隠蔽する

概念上は次の構成とする。実装時の名称は既存の `Changes` を `ChangeLog` に改名するか否かも含めて調整してよいが、責務境界は保つ。

```rust
pub(crate) struct Cursor(usize);

struct History<M: HistoryModel> {
    current: M,
    changes: Changes<M::Change>,
}

pub(crate) struct HistoryCell<M: HistoryModel> {
    history: RefCell<History<M>>,
    reader_ops: RefCell<ReaderOps>,
}
```

`ReaderOps` は現行の `RefCountOps` 相当だが、`HistoryCell` の外には公開しない。次の操作だけを `HistoryCell` から提供する。

- `snapshot(since)`：現在値と `since` 以降の履歴を参照する。
- `begin_edit()`：保留中の reader 操作を適用し、不要履歴を圧縮した後に編集ガードを返す。
- `retain_cursor(cursor)`：reader clone 用。
- `advance_cursor(old)`：旧 cursor の保持を解除し、現在の末尾 cursor を保持する。
- `release_cursor(cursor)`：reader drop 用。

cursor 操作は必要に応じて `reader_ops` に保留する。`read` は snapshot の `Ref` を呼び出し側に返した後に cursor を進めるため、その場で `history` を可変 borrow することはできない。この制約を共通型内に閉じ込める。

### 3. snapshot と編集をガード型で表現する

```rust
pub(crate) struct HistorySnapshot<'a, M: HistoryModel> {
    cell: &'a HistoryCell<M>,
    history: Option<Ref<'a, History<M>>>,
    since: Option<Cursor>,
}

pub(crate) struct HistoryEdit<'a, M: HistoryModel> {
    history: RefMut<'a, History<M>>,
    start: Cursor,
}
```

`HistorySnapshot` は次を提供する。

- 現在の `M` への参照
- reader の初回読み取りを表す `Initial` か、既読 cursor 以降の `&M::Change` iterator を持つ `Changes` かの区別

`Option<Cursor>` は `HistorySnapshot` の内部表現に留め、利用側には次の意味的な API を公開する。

```rust
pub(crate) enum HistoryDelta<I> {
    Initial,
    Changes(I),
}

impl<M: HistoryModel> HistorySnapshot<'_, M> {
    pub(crate) fn current(&self) -> &M;

    pub(crate) fn delta(
        &self,
    ) -> HistoryDelta<impl Iterator<Item = &M::Change> + '_>;
}
```

`HistoryDelta::Initial` は「変更が 0 件」ではなく「比較元の snapshot がない」ことを表す。`HistoryDelta::Changes` の iterator は空の場合があり、その場合は reader が最新位置にあり実際の変更がないことを表す。初回か否かの bool と iterator を個別に公開せず、呼び出し側がこの 2 状態を混同できない API にする。

初回読み取りを「現在要素すべての Insert」に変換する処理は Vec 固有であるため、`HistorySnapshot` ではなく `Items::changes` に残す。

`HistorySnapshot::drop` では、自分の `Ref` を先に解放した後、保留中の reader 操作と履歴圧縮の適用を試みる。他の snapshot が存在する場合は `try_borrow_mut` 失敗を正常系とし、最後の snapshot drop、次の snapshot、または次の編集開始で再試行する。これにより、現行の「次回編集開始まで不要履歴が残る」実装よりも、読み取り安全性を保ったまま解放契機を明確にできる。

`HistoryEdit` は現在値の参照、モデル実装用の可変参照、`record(change)`、開始後に履歴が増えたかの判定を提供する。State の notification と scan の dirty 判定用にこの結果を確定してから `RefMut` を解放し、snapshot と同様に保留操作の適用と履歴圧縮を試みる。reader がいない場合は、新しく追加した履歴も編集終了後にすぐ圧縮できる。

汎用層から現在値だけの無制限な `DerefMut` は公開せず、「値更新と履歴追加を同じ facade 操作内で行う」不変条件を維持する。

### 4. 履歴対応 Signal / State / Reader を共通化する

crate-private の汎用型として次を設ける。

```rust
pub(crate) struct HistorySignal<M: HistoryModel> { /* dyn HistoryNode<M> */ }
pub(crate) struct HistoryState<M: HistoryModel> { /* Rc<StateHistoryNode<M>> */ }
pub(crate) struct HistoryReader<M: HistoryModel> { /* source + Option<Cursor> */ }
```

`HistorySignal` は、履歴付き node を type erase し、次を受け持つ。

- 通常 borrow 時の reactive dependency 登録と snapshot 取得
- `from_scan(initial_model, update)` による `SourceBinder` 付き更新
- reader 生成

`HistoryState` は次を受け持つ。

- `HistoryCell<M>` の所有
- borrow 時の dependency 登録
- `borrow_mut` / `borrow_mut_loose` で取得する State 用編集ガード
- 編集ガード drop 時、履歴が増えている場合だけの通知
- `to_signal()` と `reader()`

`HistorySignal::from_scan` は更新前の cursor を記録し、update 関数の実行後に履歴が増えたかで `SinkBindings::update` の dirty 値を決める。State と scan の違いは「誰が編集を開始し、どの notification 経路を使うか」だけになる。

`HistoryReader` は現行の `SignalVecReader` と同じ契約を共通実装する。

- 未読時は `cursor == None`
- `peek` は dependency を登録して snapshot を返すが cursor を進めない
- `read` は現在 cursor で snapshot を取得し、その後に末尾へ進む
- clone は同じ cursor を追加保持し、その後の進行は独立
- drop は現在 cursor を解除

### 5. immutable source は `SignalVec` facade に残す

`&'static [T]` と `Rc<Vec<T>>` は変更履歴を持たない。これらを無理に `HistoryModel` に格納すると、ゼロコピーの slice 表現が失われ、履歴基盤に擬似 cursor の例外処理が入る。

そのため `RawSignalVec` は次の分類にする。

```rust
enum RawSignalVec<T> {
    Changing(HistorySignal<VecModel<T>>),
    Vec(Rc<Vec<T>>),
    Slice(&'static [T]),
}
```

immutable reader は facade 内の単純な unread / read フラグで現行契約を実現する。変化する source のみ `HistoryReader<VecModel<T>>` へ委譲する。

## `SignalVec` への適用

### データ構造

現行の `ItemsData<T>` を次のように分解する。

```rust
struct VecModel<T> {
    items: Vec<usize>,
    values: SlabMap<T>,
}

type VecHistory<T> = History<VecModel<T>>;
```

`ChangeData` は Vec 固有型として維持する。`ChangeData::to_signal_vec_change` は `VecModel::values` を参照し、従来どおり公開 `VecChange<'_, T>` に変換する。

### 読み取り

`Items<'a, T>` の変化 source 側は `HistorySnapshot<'a, VecModel<T>>` を保有する。

- `len` / `get` / `iter` は snapshot の現在 `VecModel` へ委譲する。
- `changes` は cursor がなければ現在要素全件を Insert として返す。
- cursor があれば、汎用履歴 iterator を `VecChange` へ変換する。
- 通常の `SignalVec::borrow` は現在の末尾 cursor を since とするため、changes を公開しない現行契約を保つ。

### State の編集

`StateVec<T>` は `HistoryState<VecModel<T>>` を包む。`borrow_mut` と `borrow_mut_loose` は汎用 State 編集ガードを Vec 固有の `ItemsMut<T>` で包み、`insert` / `remove` / `set` / `move_item` などの各操作で現在値の更新と `record(ChangeData)` を連続して行う。通知判定は `ItemsMut` ではなく汎用 State 編集ガードに集約する。

### scan の編集

`SignalVec::from_scan` は次の形で汎用 scan を構築する。

```rust
HistorySignal::from_scan(VecModel::new(), move |edit, sc| {
    let mut items = ItemsMut::from_history_edit(edit);
    f(&mut items, sc);
})
```

これにより `collections::vec` 内の `RawStateVec`、`Scan`、`ScanData`、変化 source 用の `SignalVecNode` reader ボイラープレートは不要になる。

## 変更記録の不変条件

次を必ず満たす。

1. 意味的に現在値を変える facade 操作は、同じ可変 borrow 内で 1 件以上の対応する履歴を追加する。
2. 履歴を追加しない操作（読み取り、capacity 変更、結果が同じ sort など）は dirty としない。
3. 履歴から参照される値は、対応履歴が保持されている間は安定したアドレスと内容を保つ。
4. reader cursor は保持中の履歴範囲内に必ずあり、cursor より前の履歴だけが圧縮対象になる。
5. 履歴圧縮はその履歴を参照する snapshot がない時点でだけ行う。
6. reader の clone / advance / drop 操作は遅延適用されても、適用順によって保持数が一時的に underflow しない。retain を release より先に適用する現行規則を維持する。

### `get_mut` / `IndexMut` の削除

現行の `ItemsMut::get_mut` と `IndexMut` は `&mut T` を直接返し、履歴を追加しない。これは「値変更に対応する履歴が必ずある」という共通基盤の不変条件と両立しない。

古い値を保持したまま任意の `T` へ `&mut T` を返すことは、`T: Clone` などの追加条件なしには実現できない。また、可変参照の取得後は facade が変更内容を解釈できない。そのため、追加制約や保守的な擬似履歴で既存 API を残さず、次のように削除する。

- `ItemsMut::get_mut` を削除する。
- `ItemsMut<T>` の `IndexMut<usize>` 実装を削除する。読み取り用の `Index<usize>` は維持する。
- `ItemsData::get_mut` は他に利用者がなくなるため削除する。
- `std::ops::IndexMut` の import を削除する。

要素の置き換えには、新しい `T` を渡す既存の `set(index, value)` を使う。現在値から新しい値を計算する場合も、`get` または読み取り indexing で旧値を参照し、その immutable borrow を終了してから `set` する。将来、この操作が頻出する場合は `update(index, |old: &T| -> T)` のように「旧値を借用し、新値を所有値として返す」API を別件で検討できる。初期移行では `set` に集約し、代替 API は追加しない。

これは公開 API の破壊的変更だが、現在のリポジトリ内の source、tests、examples、README に利用箇所はない。履歴の正しさを保証できない更新経路を残す互換性より、不変条件を API で強制できることを優先する。なお、`T` 自体が `Cell` / `RefCell` などの interior mutability を持つ場合の内部変更は `SignalVec` の操作履歴の対象外とする。

## モジュール構成案

初期実装では次の構成とする。

```text
src/
  history.rs                 # crate-private module 宣言と re-export
  history/
    cell.rs                  # Cursor, History, HistoryCell, snapshot/edit guard
    reader.rs                # HistoryReader
    signal.rs                # HistorySignal, scan node
    state.rs                 # HistoryState, state edit guard
  collections/vec.rs         # VecModel / ChangeData / public Vec facade
```

実装量が小さい段階では `history.rs` の単一ファイルから開始し、責務が確定した時点で分割してもよい。ファイル分割より型間の不変条件と可視性の制御を優先する。

## 実装手順

1. `Changes` と `RefCountOps` の現行動作を、複数 reader、clone、前後する advance / drop、age wrap のユニットテストで固定する。
2. `HistoryModel`、`Cursor`、`HistoryCell`、`HistorySnapshot`、`HistoryEdit` を crate-private で実装する。reader 保持数と履歴圧縮のテストは Vec に依存しない最小の scalar test model で行う。
3. `HistoryReader` を実装し、未読、peek、read、clone、drop を共通基盤のテストで固定する。
4. `HistoryState` を実装し、履歴が追加された場合だけ通知されること、loose borrow 時の schedule notify を検証する。
5. `HistorySignal::from_scan` を実装し、dependency 更新、履歴あり / なしの dirty 判定、reader 保持を検証する。
6. `ItemsData<T>` を `VecModel<T>` と汎用履歴コンテナに分解し、`Items` / `ItemsMut` を snapshot / edit guard の adapter に変更する。
7. `StateVec` を `HistoryState<VecModel<T>>` で再実装し、公開 API と serde 形式を維持する。
8. `SignalVec::from_scan` を `HistorySignal<VecModel<T>>` で再実装し、不要になった `RawStateVec` / `Scan` / 変化 source 固有の reader メソッドを削除する。
9. immutable `Vec` / slice source の経路を `RawSignalVec` の専用 variant として接続し、初回の全 Insert と 2 回目以降の空 changes を維持する。
10. `get_mut` / `IndexMut` とそれらのみが使う内部経路を削除し、無記録の値変更をできなくする。
11. 全テスト、Clippy、rustfmt、doc test を実行し、計画書を `docs/plan_archive` へ移動する。

## テスト計画

### 共通履歴基盤

- reader なしで追加した履歴が安全な圧縮契機で解放される。
- 1 つの reader の cursor 以降だけが保持される。
- 異なる cursor にいる複数 reader で、最も古い必要範囲が保持される。
- reader clone 直後は同じ cursor を保持し、一方の read が他方を進めない。
- reader drop と snapshot drop 後、不要履歴とその所有リソースが解放される。
- snapshot 生存中の advance / clone / drop で `RefCell` panic が発生せず、snapshot の内容が安定する。
- cursor の wrapping 後に age-to-index 変換が正しい。
- 履歴が 0 件の編集は dirty にならず、1 件以上追加した編集は dirty になる。

### `SignalVec` 回帰テスト

- State / scan / immutable source それぞれの初回 read、2 回目 read、peek 反復。
- State / scan の reader clone と異なる進行位置。
- insert / remove / set / move / swap / sort / drain の changes の順序と old / new 値。
- reader が遅れて読む間の複数回 set で、各中間値が改変されない。
- `set` による置き換えが `Set` を記録し、旧値と新値が異なる slab entry で保持される。
- `ItemsMut` に `get_mut` / `IndexMut` による直接の可変アクセス経路が残っていない。
- `Drop` を計測する値を使い、remove / set の旧値が最後の reader 解除まで保持され、その後に解放される。
- reactive effect が履歴のない編集で再実行されず、履歴のある編集で再実行される。
- serde のシリアライズ結果とデシリアライズ後の初回 reader 結果。

## スコープ外

- 現行の通常 `Signal<T>` / `State<T>` の公開 API の置き換え。履歴を必要としない軽量な値に履歴コストを負わせない。
- 汎用履歴型の public API 化。
- `SignalSlabMap` / `StateSlabMap` の同時移行。ただし、後続で `HistoryModel` の別実装として移行できることを設計レビューで確認する。
- history coalescing や変更列の最適化。この変更では現行の操作順と中間値を保つ。
- `Send` / `Sync` 対応。現行どおり `Rc` / `RefCell` ベースとする。

## 完了条件

- 変化する `SignalVec` の現在値、履歴、reader cursor のライフサイクルが Vec 固有 node ではなく汎用履歴基盤に実装されている。
- State と scan が同じ `HistoryCell` / `HistoryReader` を使い、Vec 側に reader 保持数の操作が残っていない。
- `SignalVec` の既存 read / peek / clone / change 契約と serde 形式が維持されている。
- `ItemsMut` facade が提供する全ての値更新経路が履歴を追加し、履歴の old / new 値が reader の保持期間中に改変または解放されない。
- 最後の reader が cursor を進めるか drop し、その履歴を参照する snapshot がなくなった後、不要な履歴とその所有リソースが解放される。
- Vec に依存しない scalar test model で共通基盤の動作が検証され、抽象化が `SignalVec` 専用になっていない。

## 実装結果

- crate-private の `history` モジュールに `HistoryModel`、`HistoryCell`、`HistorySnapshot`、`HistoryDelta`、`HistoryReader`、`HistorySignal`、`HistoryState` を実装した。
- `SignalVec` / `StateVec` は `VecModel<T>` を通じて共通履歴基盤を利用する構造へ移行した。immutable `Vec` / slice は専用 variant を維持した。
- snapshot drop と編集終了時に保留 reader 操作と履歴圧縮を再試行し、reader と snapshot のどちらからも保持されない旧値を解放するようにした。
- `ItemsMut::get_mut` と `IndexMut` を削除し、要素の置き換えを `set` に集約した。
- 履歴付き scan の下流通知を `MaybeDirty` とし、再計算で履歴が増えなかった場合は dirty を伝播しないようにした。
- `cargo nextest run --all-targets`、`cargo test --doc`、`cargo clippy --fix --allow-dirty --allow-staged --all-targets`、`cargo doc --no-deps --workspace --lib` が成功した。`cargo llvm-cov` で共通履歴基盤の主要経路を確認した。

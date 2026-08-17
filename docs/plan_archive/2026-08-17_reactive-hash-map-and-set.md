# リアクティブ HashMap / HashSet 実装計画

## 目的

`sigmut::collections` に、現在値の参照、変更差分の reader、派生 scan、およびキー単位の依存関係を備えたリアクティブな `HashMap` と `HashSet` を追加する。

## 方針

- `HashMap` を中核実装とし、`HashSet` は `HashMap<T, ()>` を包む facade として実装する。
- 現在値と変更履歴には既存の `ChangeFeedModel`、`ChangeFeedState`、`ChangeFeedStorage` を再利用する。
- コレクション全体の依存関係とは別に、キーのハッシュから作る slot を用いて `get`、`StateHashMap::contains_key`、HashSet の `contains` の依存関係を追跡する。
- ハッシュ衝突時の余分な再評価は許容するが、結果の正しさは維持する。
- キーと値を arena に保持し、変更には arena key を記録する。削除されたキーと旧値は reader が必要とする間だけ保持する。
- `K: Clone` / `V: Clone` は要求しない。
- 旧値を履歴に残せない `get_mut`、`IndexMut`、`entry`、mutable iterator は追加しない。
- 新規 unsafe code は追加しない。

## 公開 API

### `collections::hash_map`

- `StateHashMap<K, V, S = RandomState>`
- `SignalHashMap<K, V, S = RandomState>`
- `SignalHashMapReader<K, V, S = RandomState>`
- `Items<'a, K, V, S>`
- `ItemsMut<'a, K, V, S>`
- `HashMapChange<'a, K, V>`

`StateHashMap` は `new`、`with_hasher`、`insert`、`remove`、`get`、`contains_key`、`items`、`borrow_mut`、`reader`、`to_signal_hash_map` を提供する。`SignalHashMap` は `get`、`items`、`reader`、`from_scan` / `from_scan_with_hasher` を提供する。

差分は `Insert`、`Remove`、`Set` とする。初回 reader read は現在の全要素を `Insert` として返し、以降は編集順の差分を返す。

`insert` は新しいキーなら `true`、既存キーの値を置き換えた場合は `false` を返す。`remove` は要素が存在した場合だけ `true` を返す。

### `collections::hash_set`

- `StateHashSet<T, S = RandomState>`
- `SignalHashSet<T, S = RandomState>`
- `SignalHashSetReader<T, S = RandomState>`
- `Items<'a, T, S>`
- `ItemsMut<'a, T, S>`
- `HashSetChange<'a, T>`

差分は `Insert` と `Remove` とする。既存要素の再挿入は変更も通知も生成しない。

## 内部モデル

`HashMapModel<K, V, S>` は次を保持する。

- `SlabMap<Entry<K>>`: キーと現在の value key を安定した位置に保持する。
- `SlabMap<V>`: 現在値と変更履歴が参照する旧値を保持する。
- `HashMap<u64, Vec<usize>>`: full hash から active entry key を引くための index。
- `S`: borrowed lookup と index 更新に使う `BuildHasher`。

`ChangeData` は entry key、old/new value key、および通知 slot を保持する。`Set` の解放時に旧値を、`Remove` の解放時に旧値と tombstone entry を物理削除する。

## 通知

- `items` と reader は任意の変更に依存する。
- `get(key)`、`StateHashMap::contains_key(key)`、HashSet の `contains(value)` は key hash から作る slot に依存する。
- 存在しないキーへの lookup も同じ slot へ bind する。
- State 編集では ChangeFeed の全体通知後に、変更された key slot を通知する。
- scan は source invalidation を全 slot へ `MaybeDirty` として伝え、再計算で記録された変更 slot だけを dirty にし、残りを clean に戻す。

## テスト

### HashMap

- 初回、Insert、Set、Remove の delta と編集順。
- reader の peek、clone、独立 cursor。
- 最後の reader が進むまで削除キーと旧値が保持される。
- reader がない場合は履歴値が速やかに解放される。
- borrowed lookup、欠損キー lookup、キー単位と全体の通知分離。
- 意図的な hash collision でも正しい値を返す。
- scan の変更あり・変更なし更新と reader。
- batch edit が変更を記録しない場合は通知しない。

### HashSet

- 初回、Insert、Remove の delta。
- 重複 insert と欠損 remove が通知しない。
- `contains` と全体依存関係の分離。
- scan と reader が HashMap と同じ履歴契約を満たす。

## 検証

1. 対象テストを `cargo nextest run -p sigmut <filter>` で実行する。
2. `cargo fmt` を実行する。
3. `cargo clippy --fix --allow-dirty --allow-staged --all-targets` を実行する。
4. `cargo nextest run --all-targets` を実行する。
5. `cargo test --doc` を実行する。
6. `cargo doc --no-deps --workspace --lib` を実行する。
7. `cargo llvm-cov` で追加実装の未カバー経路を確認する。
8. `git diff --check` を実行する。

## 完了条件

- HashMap と HashSet が State、Signal scan、reader、差分、キー単位通知を提供する。
- HashSet の履歴・通知実装が HashMap と重複しない。
- reader が必要とする旧キー・旧値が早期解放されず、不要になった時点で解放される。
- 全検証が成功し、実装結果を追記した計画書が `docs/plan_archive` に移動されている。

## 実装結果

- `collections::hash_map` に `StateHashMap`、`SignalHashMap`、reader、`Items` / `ItemsMut`、`HashMapChange` を追加した。
- active index、entry arena、value arenaを分離し、`K: Clone` / `V: Clone`なしで削除キーと旧値をreaderのカーソル寿命まで保持するモデルを実装した。
- `get`、`StateHashMap::contains_key`、HashSetの`contains`はborrowed lookupに対応し、キーhashから作るslotへ依存関係を登録する。hash collisionでは余分な再評価だけを許容し、lookupの正しさを維持した。
- Stateのbatch editで、ChangeFeedの全体通知と変更key slotの通知を同じactionへ伝えるため、`ChangeFeedRefMut`にcrate-privateの同期通知コンテキスト連携を追加した。
- scanでは全slotへの`MaybeDirty`通知、変更slotだけのdirty確定、変更なしのclean化を実装した。
- `collections::hash_set` はHashMapの`V = ()`モデル、State、Signal、reader、editを再利用するfacadeとして実装した。重複insertは変更と通知を生成しない。
- 両State型に`Default`、`FromIterator`、serde serialize/deserializeを実装した。serialize中のmutable borrow conflictはserde errorとして返す。
- 初回・差分・peek・clone、旧値解放、同値キー再挿入、borrowed/missing lookup、キー別通知、hash collision、scan clean化、batch edit、serdeを検証する24件のテストを追加した。

## 検証結果

- `cargo fmt`: 成功
- `cargo clippy --fix --allow-dirty --allow-staged --all-targets`: 成功
- HashMap / HashSet対象 nextest: 24 passed
- `cargo nextest run --all-targets`: 287 passed、1 skipped
- `cargo test --doc`: 10 passed
- `cargo doc --no-deps --workspace --lib`: 成功
- `cargo llvm-cov --all-targets --summary-only`: 全体 line 90.05%、HashMap 88.65%、HashSet 97.74%
- `git diff --check`: 成功

Windowsのcargo commandは既存のproc-macro DLL import library生成に関する`linker_messages` warningを表示したが、Clippy diagnostic、test failure、rustdoc warningはなかった。

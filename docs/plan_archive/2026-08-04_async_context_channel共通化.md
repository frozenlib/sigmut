# async context の ContextChannel 共通化

## 目的

`AsyncSignalContext` / `AsyncActionContext` が個別に保持している raw context pointer と
有効期間管理を、同期 context channel と同じ `ContextChannel` の状態機械へ統合する。

## 方針

- `AsyncSignalContextData` は `SignalContextChannel` を保持し、source 側の context 公開を
  `scope`、async context 側の一時借用を `try_with` で行う。
- `AsyncActionContextSource` と `AsyncActionContext` は同じ `ActionContextChannel` を `Rc` で
  共有し、同様に `scope` / `try_with` を使用する。
- `SignalContextPtr` / `ActionContextPtr` は channel の内部表現に限定し、async context から
  raw pointer の生成・復元・状態更新を除去する。
- 既存 API と、context が非アクティブな場合の panic 契約は維持する。

## 検証

- context channel の対象テスト
- async signal / async action を含む全 target テスト
- doctest
- `cargo fmt`
- `cargo clippy --fix --allow-dirty --allow-staged --all-targets`
- `cargo doc --no-deps --workspace --lib`

完了後、この計画書を `docs/plan_archive` へ移動する。

## 完了結果

- async signal/action context の raw pointer と個別の状態管理を削除し、同期 channel の
  `scope` / `try_with` へ統合した。
- async action の既存テストを `AsyncActionContext::call` 経由に変更し、共通経路を直接検証した。
- format、Clippy、全 target テスト、doctest、rustdoc、coverage を完了した。
- coverage 上、今回変更した実行可能行はすべて既存テストで実行された。

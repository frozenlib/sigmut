# `Runtime` の有効 Phase 設定 実装計画

- 作成日: 2026-07-19
- 状態: 実装完了

## 背景

現在の `ActionPhase` / `ReactionPhase` は任意の `i8` から作成でき、`Action::schedule_in`、`Reaction::schedule_in`、および `Runtime` の Phase 別 dispatch API は、指定された Phase がアプリケーションで使用を許可されたものか確認しない。

さらに、Action と Reaction の待機キューは `Runtime` インスタンスではなくスレッドローカルな `Globals` にある。このため、`Runtime` が存在しない時点でも処理をスケジュールでき、その後に作成する `Runtime` の設定と矛盾する Phase がキューに残り得る。

本変更では、有効な Action Phase と Reaction Phase を `Runtime` の設定として明示できるようにし、設定中に許可されていない Phase の利用を早期に panic させる。設定付き `Runtime` の作成前にすでに該当 Phase の処理がスケジュールされていた場合も、作成時に検出して panic させる。

## 目的

1. 既存の `Runtime::new()` と互換性を保ったまま、設定を受け取るコンストラクターを追加する。
2. Action と Reaction について、有効な Phase の一覧をそれぞれ指定できるようにする。
3. `Runtime` の存在中に無効な Phase がスケジュールまたは個別 dispatch に使われた時点で panic させる。
4. `Runtime` 作成前にキューへ積まれた処理も検査し、設定上無効な Phase があれば構築を失敗させる。
5. Phase の妥当性判定を一元化し、通常 Action、Reaction、非同期 Action の wake 経路で判定漏れが起きない構造にする。

## 公開 API 方針

### 1. `RuntimeConfig` を追加する

`core` モジュールに公開設定型 `RuntimeConfig` を追加する。設定は Action と Reaction で型が異なるため、両者の有効 Phase を独立して保持する。

想定する利用形は次のとおりとする。

```rust
let config = RuntimeConfig::default()
    .with_action_phases([ActionPhase::default(), ActionPhase::new(1)])
    .with_reaction_phases([ReactionPhase::default(), ReactionPhase::new(1)]);
let mut runtime = Runtime::new_with_config(config);
```

- `RuntimeConfig::default()` は全 Phase を有効とし、現在の挙動と一致させる。
- `with_action_phases` / `with_reaction_phases` は、該当種類の許可一覧を引数の一覧で置き換える。
- 空の一覧も許可し、その種類の Phase をすべて無効にする。
- 重複要素は同じ Phase として正規化する。
- Phase ID の全範囲は `i8` と小さいため、内部表現は Phase ごとの membership 判定を定数時間で行える専用集合とし、公開 API に実装詳細を露出させない。

### 2. `Runtime::new_with_config` を追加する

```rust
pub fn new_with_config(config: RuntimeConfig) -> Self
```

- `Runtime::new()` は `RuntimeConfig::default()` を渡して共通の構築処理へ委譲する。
- `Default for Runtime` も従来どおり `Runtime::new()` と同じ無制限設定になる。
- 設定は `Runtime` の生存期間中に変更不可とする。これにより、一度有効な Phase で開始した非同期 Action が後から無効化される状態を作らない。
- 新しい設定型は `core::RuntimeConfig` として利用する。クレートルートへの再エクスポートは、現在 `Runtime` 自体がクレートルートへ再エクスポートされていないため行わない。

## 実装方針

### 1. 有効 Phase 設定を `Globals` の実行時状態として保持する

Phase 付き処理のキューとスケジュール関数はスレッドローカルな `Globals` にあるため、有効 Phase 設定も `Globals` から参照できる状態にする。

現在の `is_runtime_exists: bool` は「Runtime が存在するか」と「有効な設定」の二つを別々に管理すると不整合を作りやすい。これを、Runtime の存在中だけ `Some` になる有効設定へ置き換え、Runtime 存在判定と Phase 検証の Single Source of Truth にする。

- `Runtime::try_call` は有効設定の有無から Runtime の存在を判定する。
- `Runtime::lend` 中も設定は `Globals` に残し、`Runtime::call` 内外で同じ検証を行う。
- 所有 Runtime の終了時は設定と待機キューをまとめて消去する。
- `Runtime` が存在しない間は設定が未確定なので、スケジュール自体は従来どおり許可し、次の設定付き Runtime 作成時にまとめて検証する。

### 2. Runtime 作成を検査と状態更新に分離する

設定付き Runtime の構築では、次の順序を守る。

1. 同じスレッドに別の Runtime が存在しないことを確認する。
2. Action / Reaction の既存キューを非破壊で調べ、各非空 Phase が新しい設定で有効か確認する。
3. 無効な Phase があれば、その種類と ID が分かるメッセージで panic する。
4. 検証がすべて成功した後にだけ設定を active にし、`RawRuntime` を構築する。

検証前に Runtime の存在フラグ相当を更新すると、構築時 panic では `Runtime::drop` が走らず、スレッドが「Runtime が存在する」状態に取り残される。そのため、状態更新は検証成功後に行う。必要であれば構築途中の panic でも状態を戻せる小さな初期化ガードを用いる。

### 3. `Buckets` に非破壊の Phase 列挙を追加する

作成前のキュー検査のため、`Buckets<T>` に非空 bucket の ID を参照できる内部 API を追加する。

- キューを drain せずに非空 ID だけを列挙する。
- `start` / `last` の管理情報を利用し、キュー内容や FIFO 順を変更しない。
- `Buckets` は汎用型のままとし、`ActionPhase` / `ReactionPhase` の知識は `Globals` 側に置く。
- ID 列挙の境界、空 bucket の除外、全体が空の場合を `utils::buckets` の単体テストで確認する。

これにより、「構築検査のため一度 drain して戻す」といった順序破壊の可能性がある処理を避ける。

### 4. スケジュール経路を共通の検証関数へ集約する

`Globals` に Action / Reaction Phase の検証関数と、検証後に queue へ追加する関数を用意する。

- `Action::schedule_in` と `spawn_action_in` が通る同期 Action 経路。
- `Reaction::schedule_in` と effect 系 API が通る Reaction 経路。
- `Globals::apply_wake` が `AsyncAction` を再投入する経路。

特に `apply_wake` は現在 `actions.push` を直接呼んでいるため、共通追加関数を通すように変更する。非同期 Action の Phase は開始時から不変かつ Runtime 設定も不変だが、直接 push を残さず、すべての Phase 付き queue 書き込みで同じ不変条件を守る。

Runtime が存在する場合に無効な Phase が指定されたら、queue を変更する前に panic させる。panic メッセージには少なくとも `ActionPhase` / `ReactionPhase` の別と Phase ID を含める。

### 5. Phase 別 dispatch も検証する

次の API は、対象 queue が空の場合でも Phase の利用に当たるため、取得処理の前に設定を検証する。

- `Runtime::dispatch_action`
- `Runtime::dispatch_actions`
- `Runtime::dispatch_reactions`

`dispatch_all_actions` / `dispatch_all_reactions` / `flush` は呼び出し側が Phase を指定せず、queue への投入時または Runtime 作成時に全要素が検証済みなので、個別の Phase 検証は追加しない。

### 6. ライフサイクルと panic 後の整合性を維持する

- 所有 Runtime の drop では active 設定を解除し、既存どおり Action / Reaction queue を空にする。
- `Runtime::call` 用の一時的な非所有 `Runtime` の drop では active 設定を解除しない。
- 設定付き構築が無効な既存 Phase を見つけて panic しても、active 設定は残さない。
- 構築失敗時の既存 queue は非破壊で保持する。呼び出し側が panic を捕捉した場合、全 Phase 有効の Runtime を作って処理または drop できる。

## API ドキュメント更新

次の契約を rustdoc に記載する。

- `RuntimeConfig` の既定値は全 Action / Reaction Phase を許可すること。
- 有効一覧が Runtime ごと・Phase 種類ごとの許可リストであること。
- `Runtime::new_with_config` は、作成前にスケジュール済みの無効 Phase がある場合に panic すること。
- `schedule_in`、Phase 指定 spawn/effect API、および Phase 別 dispatch API は、active Runtime の設定で無効な Phase を指定すると panic すること。
- Runtime が存在しない間の schedule は許可され、妥当性は次の Runtime 作成時に検査されること。
- `Runtime::new()` は後方互換のため全 Phase を許可すること。

## テスト計画

### 設定と後方互換性

- `Runtime::new()` では既定 Phase、正負のカスタム Action Phase、正負のカスタム Reaction Phase を従来どおり利用できる。
- `RuntimeConfig::default()` を渡した `new_with_config` が `Runtime::new()` と同じ挙動になる。
- 設定に含めた Action / Reaction Phase は schedule と dispatch の両方で利用できる。
- 重複した Phase を設定しても挙動が変わらない。
- 空の一覧を指定すると、該当種類の既定 Phase も無効になる。

### Runtime 存在中の無効 Phase

- 無効な `ActionPhase` への `Action::schedule_in` が、queue へ追加する前に panic する。
- 無効な `ReactionPhase` への `Reaction::schedule_in` が、queue へ追加する前に panic する。
- 無効な Phase を指定した `dispatch_action` / `dispatch_actions` / `dispatch_reactions` が、対象 queue が空でも panic する。
- `spawn_action_async_in` に無効な Phase を指定した場合も、最初の Action の schedule 時点で panic する。
- `Runtime::lend` と `Runtime::call` の内側で schedule した場合も同じ設定が適用される。
- panic を捕捉した後、有効な Phase の queue と Runtime の状態が壊れていない。

### Runtime 作成前に使用済みの Phase

- Runtime がない状態で有効な Action Phase に Action を積み、設定付き Runtime を作成して dispatch できる。
- Runtime がない状態で有効な Reaction Phase に Reaction を積み、設定付き Runtime を作成して dispatch できる。
- Runtime がない状態で積んだ無効な Action Phase を、`new_with_config` が検出して panic する。
- Runtime がない状態で積んだ無効な Reaction Phase を、`new_with_config` が検出して panic する。
- 有効・無効な複数 Phase が混在する場合も、無効なものを見落とさない。
- 作成時 panic の後に active Runtime 設定が残らず、全 Phase 有効の Runtime を新たに作成できる。
- 作成時検査で queue の内容と同一 Phase 内の FIFO 順が変化しない。

### 内部 queue

- 空の `Buckets` からは Phase ID が列挙されない。
- 複数 ID のうち非空 bucket だけが列挙される。
- 一部 drain / pop 後に空になった bucket は列挙されない。
- 負数、0、正数の ID を正しく列挙する。

## 検証手順

実装後、スキルの指針に従って次を順に実行する。

1. 追加した `Buckets` と `core` の対象テストを `cargo nextest run` で実行する。
2. `cargo fmt` を実行する。
3. `cargo clippy --fix --allow-dirty --allow-staged --all-targets` を実行する。
4. `cargo nextest run --all-targets` を実行する。
5. `cargo test --doc` を実行する。
6. `cargo doc --no-deps --workspace --lib` を実行し、公開 API のリンクと警告を確認する。

失敗時は個別経路に場当たり的な例外を追加せず、active 設定の所在、queue 追加の共通化、または構築順序の不変条件を修正する。

## 対象外

- `ActionPhase::new` / `ReactionPhase::new` 自体での検証。Phase は Runtime ごとに有効性が異なり、Runtime 作成前にも値を定義できる必要がある。
- Runtime 生存中の有効 Phase 設定の変更。
- 無効な Phase を `Result` として返す fallible API の追加。今回の要求どおりプログラミングエラーとして panic させる。
- Action Phase と Reaction Phase の単一型への統合。
- Phase の実行順序や `dispatch_all_*` の順序変更。

## 完了条件

- `Runtime::new()` の既存利用が変更なしで動作する。
- 設定付きコンストラクターから有効な Action / Reaction Phase 一覧を指定できる。
- active Runtime の設定にない Phase を schedule または個別 dispatch に使うと、queue 変更前に panic する。
- Runtime 作成前に積まれた無効な Phase を、設定付き Runtime の作成時に非破壊で検出して panic する。
- 同期 Action、Reaction、非同期 Action wake の全 queue 追加経路が共通検証を通る。
- 構築時 panic や `Runtime::call` を含むライフサイクルで、active 設定が残留・消失しない。
- 対象テスト、全テスト、doc test、Clippy、rustdoc がすべて成功する。

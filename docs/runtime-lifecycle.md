# Phase と Runtime の寿命契約

## Phase の定義と順序

```rust
use sigmut::{ActionPhase, ReactionPhase, core::{Runtime, RuntimeConfig}};

static COMMIT: ActionPhase = ActionPhase::new();
static UPDATE: ReactionPhase = ReactionPhase::new();
static PRESENT: ReactionPhase = ReactionPhase::new();

let mut runtime = Runtime::new_with_config(RuntimeConfig::default()
    .with_action_phases([&COMMIT])
    .with_reaction_phases([&UPDATE, &PRESENT]));
```

Phase は `static` で定義し、API には `&'static ActionPhase` /
`&'static ReactionPhase` を渡す。`new()` は `const fn` だが、所有する
`LazyLock<usize>` を利用箇所ごとに展開する `const PHASE` は使わない。
初回使用時のグローバル採番は同一性だけを表し、実行順序や永続化に使わない。
ID は再利用せず、連番を使い切ると衝突する前に panic する。

全 phase の dispatch は設定した一覧の順に行い、phase 内は FIFO とする。
dispatch の各 batch は開始時の件数だけを記録し、呼出し直前に一件ずつ取り出す。
callback の panic でも後続の予約は元の queue に残る。実行中に失敗した callback 自体を
自動 replay する契約はない。batch 中の新規予約は次の batch / dispatch で扱う。
重複登録は panic。空の一覧も有効。一覧指定で default を暗黙追加しない。
`Runtime::new()` / `RuntimeConfig::default()` は、型ごとの単一定義である
`ActionPhase::DEFAULT` と `ReactionPhase::DEFAULT` だけを扱う。
`Default` は静的参照にも実装し、同じ定義を返す。

設定の構築は、最大登録 ID + 1 の対応表と、設定順の ID 一覧を確保する。
以後の ID → dense local index は最悪 O(1) で、範囲外 ID は非対応となり、表を伸長しない。
TLS の Reaction queue の初回確保・拡張や Runtime 開始時の対応 phase の件数集計は、
定常時の lookup / ready 判定とは別のコストである。全 dispatch は対応 phase だけを走査する。

## 所有者と持越し

| 処理 | 所有・終了時の扱い |
| --- | --- |
| State / Signal の値 | オブジェクトの所有権に従う |
| Action / AsyncAction / inline async action | 現 Runtime session。Closing で未実行分・継続を取消し |
| Reaction | PhaseID ごとの同じ TLS queue に常設。開始・終了時に移送しない |
| effect / effect_async | Subscription が所有。queue は weak target で owner 消滅を検出 |
| Signal 保持 Future / Stream | Signal と既存の discard 条件に従う。Runtime 終了だけでは取消さない |
| notify / unbind / discard / stream 更新予約 | 対象オブジェクトの保守として TLS に保持 |
| wake | 持続 target 向けは保持。取消済み Action 向けは無効化 |
| driver の待機 Waker | Closing 開始で解除。次 Runtime へ引き継がない |

Runtime の不在・非対応 phase・Closing のどの場合も Reaction の登録を受理する。
Running Runtime は対応 phase だけを実行し、非対応 phase は休眠する。
A → B → A の切替では A の予約を保持したまま B を起動でき、A 復帰時に再開する。
B による共有 State の変更通知も失わず、A の effect は復帰時の最新値を読む。

Reaction の ready 判定は現在の対応 phase の実行可能件数を使って O(1) で行う。
休眠 queue だけでは `wait_for_ready` / `flush` を回し続けない。
Action と通知・解除・discard・wake の内部保守は別に判定する。
`flush` は保守から新たに作られた Action も含め、同じ ready 判定で settle を確認する。
Closing 中の `wait_for_ready` は Pending で、待機 Waker を保持しない。
Signal 保持 Future / Stream を直接読む場合も poll は停止し、保持値を返す。
Closing 中に未初期化の async Signal を読んでも Future を開始しない。

Phase は graph / session isolation ではない。共通 default を扱う Runtime 間では
default の Reaction が実行され得る。専用環境の処理には専用 phase を使う。
任意の Reaction closure は、対応 Runtime が来るまで捕捉値を保持し得る。
owner の終了で解放すべき処理は `Reaction::from_weak_fn` などを使う。
テストや利用側は Subscription を破棄し、必要な解除・discard を Running 中に flush する。

## Action と Closing

| 状態 | 明示的な schedule / start |
| --- | --- |
| Runtime 不在・Drop 後 | 契約違反として panic |
| Running | 対応 ActionPhase は受理。非対応 phase は panic |
| Closing | phase にかかわらず実行・初回 poll をせず取消し |

未登録の `Action::new` などの recipe 構築には Runtime は不要。
`Runtime::begin_shutdown(&mut self)` は dispatch 外の制御側で呼ぶ。
Running → Closing は不可逆・冪等で、呼ばずに Drop した場合も同じ手順を通る。
実行中の同期 callback / poll を途中破棄する操作ではない。

終了手順は、先に TLS binding を Closing にし、Action queue / Future を内部 borrow から
取り外して、capture / Future を borrow 外で破棄する。非同期 Action の破棄中は元の
`AsyncActionContext` を使用できる。Drop 内の新しい Action は取消し、Reaction / cleanup の
登録は保持する。Closing の通常 dispatch / poll は停止するので、取消し → Reaction 再予約
→ 再取消しの反復は起きない。binding は Runtime 自体の Drop まで残る。

実行済みの State 変更は巻き戻さず、通知も保持する。
非同期 Action の wake 登録は Future の破棄前に無効化・解除する。
既に queue に入った wake も同じ有効性 token を検査し、登録枠が再利用されても新しい
target へ接続しない。旧 wake は新 driver を起こさず、旧 AsyncActionContext は終了後に
新 Runtime の context を取得できない。

Action / spawn / inline start の戻り値は `()` であり、sigmut 固有の未完了 join handle はない。
capture の破棄による既存 channel の取消し通知を妨げない。
持続 Stream / Future が対応 phase 待ちで Pending であることは Action の取消しとは異なる。
rt-local の Task API や戻り値は変更しない。

Waker の clone は内部 borrow / Mutex 内で行える前提とする。
wake / Drop は scheduler の TLS borrow / wake request Mutex の外で行う。
driver への通知、oneshot の通知・Waker 差し替え・登録解除、stream の待機 Waker も同様。
effect_async / Signal 保持 Future の完了・差し替え・discard で内部が Future を破棄するときは、
ノードのデータ borrow を外す。ユーザー callback / poll 自身の実行中は既存の循環依存検出が適用される。

## 永続 bridge と外部資源

値の導出には派生 Signal を使う。永続 effect が Action で別 State にミラーする場合は、
effect の実行完了だけでは更新の適用完了にならない。owner は未反映 revision を保持し、
Action が実際に適用した revision だけを ack する。未 ack の取消しでは、その bridge の
Reaction だけを再予約する。Action 実行時に最新の値・revision を読む方式では、後続更新の
誤 ack と古い snapshot の再適用を避けられる。owner 消滅時には weak target で終了する。

sigmut の production source には effect → Action の永続 State mirror は存在しない。
公開 API だけを使う検証用 bridge を
[`tests/phase_lifecycle/bridge.rs`](../tests/phase_lifecycle/bridge.rs) に置き、未反映取消し、
Closing 中の休眠、別 Runtime 中の更新、適用 revision の ack、owner 消滅を検証する。
一般 effect の一律 replay や、実行済み外部副作用の再実行は保証しない。

Future の保持は、それが捕捉する Tokio runtime / UI handle / native session の復活を意味しない。
外部資源に依存する Subscription / Signal は、資源の owner が終了・discard させる。
終了に必須の非同期 cleanup は、Closing に入る前に完了させる。

## acoui 側の採用時の移行

- 数値付き Phase / const の定義を static と参照へ変更し、全 Runtime に必要な phase の順序を指定する。
- default を使う場合は明示的な一覧にも `ActionPhase::DEFAULT` / `ReactionPhase::DEFAULT` を含める。
- Runtime より前の Action 登録をなくす。Drop 由来の解除は Action でなく持続 cleanup として扱う。
- 終了待ち後、通常処理を挟まず `begin_shutdown` を呼ぶ。必須の保存・反映・非同期 cleanup はその前に完了する。
- 永続 State bridge（StateSinkEntry 等）に未反映 revision / 適用時 ack / 取消し ticket を実装する。
- session 固有の Subscription と外部資源を持つ Future は owner が終了させる。default の共有を隔離と解釈しない。
- settle は対応 phase の ready と必要な保守を使い、休眠 queue の残存を終了阻害として扱わない。

この実装は sigmut だけを変更する。acoui / rt-local の改修・採用検証は行っていない。

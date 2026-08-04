# 同期 context channel の追加

## 目的

同期的なnative callbackを呼ぶ間だけ既存のcontextを公開し、同期的な再入先から安全に復元できる低レベルAPIを追加する。

対象は次の3型とする。

- `SignalContextChannel`
- `ReactionContextChannel`
- `ActionContextChannel`

## 公開API

各Channelは送信側と受信側に分割せず、inlineな単一型として次を持つ。

- `new`: 非アクティブなChannelを作る。
- `scope`: `&mut Context`をcallback実行中だけ利用可能にし、終了時に以前の状態へ戻す。
- `try_with`: 利用可能なcontextをcallback固有のライフタイムで一時借用する。非アクティブなら`None`、既に借用中ならpanicする。

`scope`は字句的な利用可能期間とネスト時の復元を表すために選ぶ。`try_with`は非アクティブを通常の分岐として扱い、mutable aliasにつながる直接再入だけを契約違反として区別するために選ぶ。

## 内部表現

共通のinline状態機械を`Cell`で保持する。

```text
Inactive
Available(raw context pointer)
Borrowed
```

- Channelは`Rc`、`Box`、heap上の状態stackを持たない。
- `scope`は以前の状態をスタック上のRAII guardへ保存し、新しい`Available`へ置換する。
- `try_with`は`Available`を`Borrowed`へ置換し、callback終了時にRAII guardで同じ`Available`へ戻す。
- guardは正常終了とunwindの双方で復元する。
- zero-sized markerによりruntimeのthread-local/exclusive規約に合わせて`!Send + !Sync`とする。

raw pointerの生成・context復元はprivate型へ集約する。既存の`SignalContextPtr`、`AsyncSignalContextSource`、`AsyncActionContextSource`/`AsyncActionContext`も同じ変換を使い、unsafeなfield再構築を重複させない。

`SignalContextChannel::try_with`と`ReactionContextChannel::try_with`は`SignalContext`内部ライフタイムをcallbackごとのHRTBで生成する。`ActionContextChannel::try_with`も`&mut ActionContext`のライフタイムをcallbackへ閉じ込める。返り値型はこれらのライフタイムの外側にあるため、`StateRef`や`StateRefMut`をcallback外へ返せない。

## テスト

各Channelについて次を確認する。

1. 非アクティブ時は`None`でcallbackを呼ばない。
2. 通常の`scope`でcontextを利用できる。
3. 同じ`scope`内で複数回`try_with`できる。
4. `try_with` callbackからの直接`try_with`はpanicし、元の借用状態を壊さない。
5. `try_with`中に同じcontextを`scope`で明示的にreborrowすると、同期再入先の`try_with`が成功し、終了後は外側の`Borrowed`へ戻る。
6. `scope`と`try_with`のpanicをcatchした後も以前の状態へ戻り、終了後にdangling pointerを保持しない。
7. compile-fail doctestで`StateRef`/`StateRefMut`がcallback外へ脱出しないことを確認する。
8. 既存async contextを含む全テストを実行する。

## 検証

- `cargo fmt`
- `cargo clippy --fix --allow-dirty --allow-staged --all-targets`
- Channelの対象テスト
- `cargo nextest run --all-targets`
- `cargo test --doc`
- `cargo doc --no-deps --workspace`
- `cargo llvm-cov`で追加コードの分岐を確認

完了後、この計画書を`docs/plan_archive`へ移動する。

## 完了結果

- 3種類のinline Channelと`scope`/`try_with`を実装した。
- raw context変換をprivate型へ集約し、既存async signal/action contextと共有した。
- 状態遷移、権限保持、直接再入拒否、明示的reborrow、正常終了/unwind復元をテストした。
- compile-fail doctestで`StateRef`、reaction経由の`StateRef`、`StateRefMut`の脱出を拒否した。
- format、Clippy、全targetテスト、doctest、rustdoc、coverageを完了した。

# sigmut

[![Crates.io](https://img.shields.io/crates/v/sigmut.svg)](https://crates.io/crates/sigmut)
[![Docs.rs](https://docs.rs/sigmut/badge.svg)](https://docs.rs/sigmut/)
[![Actions Status](https://github.com/frozenlib/sigmut/workflows/CI/badge.svg)](https://github.com/frozenlib/sigmut/actions)

`sigmut` は、UI フレームワークの基盤として使うことを想定した状態管理フレームワークです。

> [!WARNING]
> 警告: このクレートはまだ開発の非常に初期段階です。API は変更される可能性が高く、ドキュメントもまだ少ない状態です。

## 特徴

- [シグナルベースの API](#シグナルベースの-api)
- [「状態の変更」と「状態の計算」の分離](#「状態の変更」と「状態の計算」の分離)
- [扱いやすいシングルスレッドモデル](#扱いやすいシングルスレッドモデル)
- [`async`/`await` を使った非同期処理のサポート](#asyncawait-を使った非同期処理のサポート)
- [グリッチ無し (古い状態に基づく不要な計算を行わない)](#グリッチ無し-古い状態に基づく不要な計算を行わない)
- [より効率的なリアクティブプリミティブを実装可能](#より効率的なリアクティブプリミティブを実装可能)

### シグナルベースの API

`sigmut` では、次のリアクティブプリミティブを使って状態管理を行います。

- [`State<T>`] : `Rc<RefCell<T>>` に似ていますが、変更を監視する機能が追加されています。
- [`Signal<T>`] : `Rc<dyn Fn() -> &T>` に似ていますが、結果の変更を監視する機能が追加されています。
- [`effect`] : 依存している状態に変更があったときに再度呼び出される関数です。

[`State<T>`]: https://docs.rs/sigmut/latest/sigmut/struct.State.html
[`Signal<T>`]: https://docs.rs/sigmut/latest/sigmut/struct.Signal.html
[`effect`]: https://docs.rs/sigmut/latest/sigmut/fn.effect.html

```rust
use sigmut::{Signal, State};

let mut rt = sigmut::core::Runtime::new();

let a = State::new(0);
let b = State::new(1);
let c = Signal::new({
    let a = a.clone();
    let b = b.clone();
    move |sc| a.get(sc) + b.get(sc)
});
let _e = c.effect(|x| println!("{x}"));

rt.flush(); // "1" を表示

a.set(2, rt.ac());
rt.flush(); // "3" を表示

a.set(3, rt.ac());
b.set(5, rt.ac());
rt.flush(); // "8" を表示
```

状態間の依存関係は自動的に追跡され、変更が発生すると再計算も自動的にトリガーされます。

この仕組みは近年のトレンドであり、次のような他の状態管理ライブラリでも採用されています。

- [SolidJS](https://docs.solidjs.com/advanced-concepts/fine-grained-reactivity)
- [Preact Signals](https://preactjs.com/guide/v10/signals/)
- [JavaScript Signals standard proposal](https://github.com/tc39/proposal-signals)
- [Svelte runes](https://svelte-5-preview.vercel.app/docs/runes)

### 「状態の変更」と「状態の計算」の分離

多くの状態管理ライブラリは、状態の変更と状態の計算を分離することでロジックを単純にしています。

[Elm] では、[Model-View-Update] アーキテクチャによって状態の変更 (Update) と状態の計算 (View) を分離しています。

[Elm]: https://elm-lang.org/
[Model-View-Update]: https://guide.elm-lang.org/architecture/

[React] では、[`Components and Hooks must be pure`] というルールによって、状態の計算中に状態を変更することが禁止されています。React の [StrictMode] では、このルールが守られていることを確認するために状態の計算が追加で呼び出されます。

[React]: https://react.dev/
[`Components and Hooks must be pure`]: https://react.dev/reference/rules#components-and-hooks-must-be-pure
[StrictMode]: https://react.dev/reference/react/StrictMode#fixing-bugs-found-by-double-rendering-in-development

[SolidJS] では、依存する計算が状態更新の完了後に実行されるように更新がバッチ化されます。これは [`batch(fn)`](https://docs.solidjs.com/reference/reactive-utilities/batch) によって明示的に行われる場合も、フレームワークによって暗黙的に行われる場合もあります。これにより、システム全体で状態の変更と状態から導かれる計算が分離された状態に保たれます。

[SolidJS]: https://www.solidjs.com/

`sigmut` では、[`SignalContext`] と [`ActionContext`] を使って状態の変更と状態の計算を分離します。

- [`ActionContext`] : 状態の変更に使います
- [`SignalContext`] : 状態の計算に使います

[`ActionContext`]: https://docs.rs/sigmut/latest/sigmut/struct.ActionContext.html
[`SignalContext`]: https://docs.rs/sigmut/latest/sigmut/struct.SignalContext.html

状態の変更や状態の計算を行う関数に、それぞれ対応するコンテキストの使用を要求することで、状態の変更と状態の計算の区別が明確になり、コンパイラがその分離を強制できます。

「状態の変更と状態の計算の分離」は、状態の計算中に状態を不変として扱うことでロジックを単純にします。これは Rust の所有権の考え方に似ています。内部的に `sigmut` は [`RefCell`] を使っていますが、この類似性によって状態計算中の [`BorrowError`] を避けやすくなります。多くの `Rc<RefCell<T>>` を使っている場合、`sigmut` に切り替えることで [`BorrowError`] の発生が少ない、より堅牢なコードベースにできます。

[`RefCell`]: https://doc.rust-lang.org/std/cell/struct.RefCell.html
[`BorrowError`]: https://doc.rust-lang.org/std/cell/struct.RefCell.html#method.borrow

### 扱いやすいシングルスレッドモデル

`sigmut` は、次の理由からシングルスレッドモデルを採用しています。

- シンプルで扱いやすい
- デッドロックのリスクがない
- 同期が不要なため、現在値を即座に取得できる
- `async/await` との相互運用により、マルチスレッドの利点も活用できる
- グリッチ無しにできる (古い状態に基づく不要な計算を行わない)

### `async`/`await` を使った非同期処理のサポート

`sigmut` は `async`/`await` と統合されており、非同期処理を同期的な [`Poll<T>`] 状態として扱えます。これにより、[`tokio`] のような非同期ランタイムとの相互運用が可能になります。

[`Poll<T>`]: https://doc.rust-lang.org/std/task/enum.Poll.html
[`tokio`]: https://tokio.rs/

詳細については、名前に [`async`]、[`future`]、または [`stream`] を含む関数や型を参照してください。

[`async`]: https://docs.rs/sigmut/latest/sigmut/struct.ActionContext.html?search=async
[`future`]: https://docs.rs/sigmut/latest/sigmut/struct.ActionContext.html?search=future
[`stream`]: https://docs.rs/sigmut/latest/sigmut/struct.ActionContext.html?search=stream

### グリッチ無し (古い状態に基づく不要な計算を行わない)

一部の状態管理ライブラリでは、状態の計算中に古いキャッシュを使用することがあり、それによって予期しない結果が生じる場合があります。こうした予期しない結果はすぐに再計算され、意図しない計算結果は破棄されますが、予期しない結果によってユーザーコードでのパニックを発生させる可能性もあります。
そのため、再計算が行われるというだけでは、この問題が完全に解決されるわけではありません。

`sigmut` では、キャッシュを `clean`、`dirty`、`maybe dirty` の 3 種類に分類して管理し、キャッシュの妥当性を常に正確に確認することで、ユーザーからは常に最新の状態に依存した値のみが見えるようにしています。この動作により、ユーザーは古い状態と最新の状態の混在について考慮する必要がなくなり、コードはシンプルかつ堅牢になります。

### より効率的なリアクティブプリミティブを実装可能

`sigmut` には、状態変更の通知だけを扱う低レベルモジュール [`sigmut::core`] が含まれています。このモジュールを使うことで、特定の条件下でより効率的なリアクティブプリミティブを実装できます。

その実装例が [`SignalVec<T>`] です。[`SignalVec<T>`] は [`Signal<Vec<T>>`] に似ていますが、前回アクセスしてからの変更履歴を取得できるため、より効率的な処理が可能になります。

[`sigmut::core`]: https://docs.rs/sigmut/latest/sigmut/core/index.html
[`SignalVec<T>`]: https://docs.rs/sigmut/latest/sigmut/collections/vec/struct.SignalVec.html
[`Signal<Vec<T>>`]: https://docs.rs/sigmut/latest/sigmut/struct.Signal.html

## ライセンス

このプロジェクトは Apache-2.0/MIT のデュアルライセンスです。詳細については 2 つの LICENSE-\* ファイルを参照してください。

## コントリビューション

Apache-2.0 ライセンスで定義されるとおり、あなたが本プロジェクトへの取り込みを目的として意図的に提出したコントリビューションは、明示的に別の意思表示をしない限り、追加の条件なしで上記と同じデュアルライセンスの下で提供されるものとします。

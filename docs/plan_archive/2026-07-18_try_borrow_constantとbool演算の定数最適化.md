# `try_borrow_constant` と bool 演算の定数最適化 実装計画

- 作成日: 2026-07-18
- 状態: 実装完了

## 背景

現在の `Signal::try_borrow_constant` は `RawSignal::StaticRef` だけを定数として扱い、`Signal::from_value`、`Signal::from_value_map`、`Signal::from_owned` が生成する `ConstantNode` を認識しない。

また、`Signal<bool>` の `BitOr` / `BitAnd` は、結果が定数になる場合を一部簡約するが、次の恒等則は適用していない。

- `false | x = x`
- `x | false = x`
- `true & x = x`
- `x & true = x`

本変更では、ノード自身が定数参照を提供できる仕組みを追加し、bool 演算では通知特性の維持よりも論理的同一性と効率を優先して、恒等元との演算結果に既存のオペランドを直接再利用する。

## 目的

1. `SignalNode` が定数性を保守的に申告できるようにする。
2. `DynSignalNode` を経由して、型消去後も定数参照を取得できるようにする。
3. `ConstantNode` を `Signal::try_borrow_constant` から認識できるようにする。
4. `Signal<bool>` の `BitOr` / `BitAnd` に、吸収則・定数畳み込み・恒等則を適用する。
5. 定数判定と演算子最適化の契約をテストとドキュメントで明確にする。

## 実装方針

### 1. `SignalNode` に定数参照取得メソッドを追加する

`SignalNode` に次の既定メソッドを追加する。

```rust
fn try_borrow_constant(&self) -> Option<&Self::Value> {
    None
}
```

- `SignalNode` は公開トレイトなので、既存の外部実装を壊さないよう既定値を `None` とする。
- `Some` は現在値のスナップショットではなく、ノードの生存期間中に出力が変化せず、リアクティブな評価や依存関係の登録を必要としないことを表す。
- 判定できないノードは、実際には値が変化しない場合でも `None` を返してよい保守的なAPIとする。

### 2. `DynSignalNode` に型消去用の委譲メソッドを追加する

既存の命名規則に合わせ、`DynSignalNode` に `dyn_try_borrow_constant` を追加する。

`impl<S: SignalNode> DynSignalNode for S` では、`SignalNode::try_borrow_constant` に委譲する。これにより、`RawSignal::Node(Rc<dyn DynSignalNode<...>>)` から各具象ノードの実装を呼び出せるようにする。

### 3. `ConstantNode` で定数参照を返す

`ConstantNode` の `SignalNode` 実装で `try_borrow_constant` をオーバーライドし、保持している値に射影関数を適用した参照を返す。

```rust
fn try_borrow_constant(&self) -> Option<&Self::Value> {
    Some((self.map)(&self.value))
}
```

これにより、以下のコンストラクターで作成したシグナルを定数として認識できるようにする。

- `Signal::from_value`
- `Signal::from_value_map`
- `Signal::from_owned`

`StateNode`、scan、future、stream、async、および任意の `FromBorrowNode` は定数性を保証できないため、既定の `None` のままとする。

### 4. `Signal::try_borrow_constant` をノードへ委譲する

`RawSignal::Node` の場合に即座に `None` を返す現在の実装を、`DynSignalNode::dyn_try_borrow_constant` へ委譲する実装に変更する。

- `RawSignal::StaticRef` は従来どおり `Some` を返す。
- 通常の `Signal::borrow` の挙動や依存関係登録は変更しない。

### 5. `Signal<bool>` の論理演算を完全に簡約する

参照を保持したまま `self` / `rhs` をムーブしないよう、両オペランドの定数判定結果を `.copied()` で `Option<bool>` に変換してから分岐する。

`BitOr` は次の順序で処理する。

1. どちらかが `true` なら `Signal::TRUE`。
2. 両方が `false` なら `Signal::FALSE`。
3. 左辺だけが定数 `false` なら `rhs` をそのまま返す。
4. 右辺だけが定数 `false` なら `self` をそのまま返す。
5. 両方とも非定数なら、従来どおり `Signal::new_dedup` で派生シグナルを作る。

`BitAnd` は次の順序で処理する。

1. どちらかが `false` なら `Signal::FALSE`。
2. 両方が `true` なら `Signal::TRUE`。
3. 左辺だけが定数 `true` なら `rhs` をそのまま返す。
4. 右辺だけが定数 `true` なら `self` をそのまま返す。
5. 両方とも非定数なら、従来どおり `Signal::new_dedup` で派生シグナルを作る。

恒等元の簡約では、結果の通知特性は新しい `new_dedup` ノードではなく、再利用されたオペランドの通知特性を引き継ぐ。これは意図した仕様変更として扱う。

## ドキュメント更新

次の契約をAPIドキュメントに記載する。

- `SignalNode::try_borrow_constant` の `Some` は永続的な定数性の宣言であること。
- `None` は非定数であることを断定せず、判定不能も含むこと。
- `Signal::try_borrow_constant` はリアクティブな評価や依存関係登録を行わないこと。
- `from_value_map` の射影関数は、定数ノードとして扱える安定した射影であり、呼び出し回数や呼び出し時点に依存する副作用を持たせないこと。
- bool 演算は恒等元との演算時にオペランド自身を再利用し、その通知特性を維持すること。

## テスト計画

### 定数判定

- `Signal::from_static_ref` が `Some` を返す。
- `Signal::from_value` が `Some` を返す。
- `Signal::from_value_map` が射影後の参照を `Some` で返す。
- `Signal::from_owned` による `str` などの `?Sized` な出力でも `Some` を返す。
- `State::to_signal` が `None` を返す。
- 通常の `Signal::new` が、クロージャの結果が常に同じ値でも `None` を返す。
- 独自 `SignalNode` のオーバーライドが `DynSignalNode` 経由で `Signal::try_borrow_constant` に反映される。

### `BitOr`

- `TRUE | x` と `x | TRUE` が `Signal::TRUE` に正規化される。
- `FALSE | FALSE` が `Signal::FALSE` に正規化される。
- `FALSE | x` と `x | FALSE` が `Signal::ptr_eq` で `x` と同一になる。
- 両辺が非定数の場合は新しい派生シグナルになり、値の変化を正しく反映する。
- `Signal::from_value(true/false)` でも同じ簡約が働く。

### `BitAnd`

- `FALSE & x` と `x & FALSE` が `Signal::FALSE` に正規化される。
- `TRUE & TRUE` が `Signal::TRUE` に正規化される。
- `TRUE & x` と `x & TRUE` が `Signal::ptr_eq` で `x` と同一になる。
- 両辺が非定数の場合は新しい派生シグナルになり、値の変化を正しく反映する。
- `Signal::from_value(true/false)` でも同じ簡約が働く。

### 代入演算子

`derive_ex` が生成する `BitOrAssign` / `BitAndAssign` についても、代表ケースで同じ簡約とポインター同一性が得られることを確認する。

## 検証手順

実装後、次を順に実行する。

1. `cargo fmt --all -- --check`
2. `cargo test --workspace`
3. `cargo clippy --workspace --all-targets -- -D warnings`

失敗した場合は警告やテストを局所的に回避せず、定数性の契約、所有権、または演算分岐の根本原因を修正する。

## 対象外

今回の変更では、次の追加最適化は行わない。

- `Signal::map` が生成する `FromBorrowNode` への定数性伝播。
- `KeepNode` への定数性伝播。
- クロージャやリアクティブ依存関係を解析した定数推論。
- bool 以外の演算子への定数畳み込みの展開。

これらは必要性と通知・ライフタイム上の契約を個別に検討した上で、別の変更として扱う。

## 完了条件

- 公開 `SignalNode` トレイトへの追加が既定実装付きで行われている。
- `DynSignalNode` を介して `ConstantNode` の定数参照を取得できる。
- `Signal::from_value`、`from_value_map`、`from_owned` が定数として認識される。
- bool の吸収則、定数畳み込み、恒等則が左右対称に適用される。
- 恒等元との演算結果が動的オペランドとポインター同一になる。
- 非定数同士の演算は従来どおりリアクティブに更新される。
- フォーマット、テスト、Clippyがすべて成功する。

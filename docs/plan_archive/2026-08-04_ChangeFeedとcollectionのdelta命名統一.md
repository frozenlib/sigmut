# ChangeFeed と collection の delta 命名統一 計画

## 目的

`change` を1件の具体的な変更記録、`delta` を基準点から現在までを再現する change 列、`current` を現在値として、ChangeFeed と collection API の語彙を統一する。

## 実装

1. `ChangeFeedDelta::Changes` を `Incremental` に改名し、module docs、public docs、doctest、crate 内の全利用箇所を更新する。
2. `ChangeFeedRefMut::changes` を既存の edit 開始 cursor から実装し、この edit で記録済みの change だけを記録順に公開する。
3. `SignalVec` と `SignalSlabMap` の reader 結果が公開する高水準 change 列を `changes` から `delta` に改名する。通常の collection 列挙や内部履歴操作は変更しない。
4. 初回・incremental・peek/read の既存 semantics と、mutable edit の change 範囲・順序・current との整合性をテストする。
5. format、Clippy、全テスト、doctest、rustdoc を実行し、成功後に本計画書を `docs/plan_archive` へ移してコミットする。

## 対象外

- `ChangeFeedRefMut::is_dirty` の公開
- `ChangeFeedRevision`、untracked read/borrow API の追加
- `VecChange` など1件の change を表す型の改名
- TextDocument / acoui の変更

## 実装結果

- `ChangeFeedDelta::Changes` を `Incremental` に改名し、docs、doctest、crate 内の全 call site を更新した。
- `ChangeFeedRefMut::changes` を edit 開始 cursor から実装し、空編集、記録順、開始前の change の除外、current との整合性をテストした。
- `SignalVec` と `SignalSlabMap` の reader 結果 API を `delta` に統一し、初回・incremental・peek/read・clone の既存 semantics を維持した。
- format、Clippy、全 target の check、全テスト、doctest、Rustdoc、coverage が成功した。

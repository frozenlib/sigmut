# `SignalVecReader::clone` 実装計画

- 作成日: 2026-07-20
- 状態: 実装完了

## 目的

`SignalVecReader` をclone可能にし、clone時点では同じcursor位置を共有しつつ、その後の `read` では各readerが独立してcursorを進められるようにする。

## 実装方針

1. `SignalVecReader` に手動の `Clone` 実装を追加する。cursor未設定時はsourceと未設定状態だけを複製する。
2. cursor設定済みの場合は、clone先が同じ履歴位置を保持できるようにsourceへreader保持操作を追加する。
3. `RefCountOps` に任意ageの参照カウント増加を遅延適用する操作を追加する。既存の末尾へのreader移動は現在の集約処理を維持する。
4. immutable slice / `Vec` は履歴を持たないため保持操作をno-opとし、cursor `0` を各readerが独立して保持する。
5. rustdocに、clone直後は同じcursor位置であり、その後は独立して進むことを記載する。

## テスト計画

- `StateVec`: cursor未設定のcloneが元readerの初回 `read` に影響されない。
- `StateVec`: 履歴途中でclone後、一方を進めてsourceを更新しても、他方がclone時点以降の全changesを取得できる。
- `SignalVec::from_scan`: 上流更新とscan再評価を挟んでも、cloneしたreaderごとのchangesが保持される。
- immutable slice / `Vec`: clone元の `read` 後もclone先が初回Insert列を取得できる。
- `RefCountOps`: 任意ageのretainと一方のrelease後も履歴が残り、最後のrelease後に掃除できる。

## 検証

1. 追加した対象テストを実行する。
2. `cargo fmt` と自動修正付きClippyを実行する。
3. 全targetテスト、doc test、coverage、rustdoc、厳格Clippyを実行する。

## 完了条件

- `SignalVecReader: Clone` が全source種別で利用できる。
- cloneされたreaderのcursorと履歴寿命が独立する。
- 対象テストと全体検証が成功する。
- 本計画書を `docs/plan_archive` へ移し、変更をコミットする。

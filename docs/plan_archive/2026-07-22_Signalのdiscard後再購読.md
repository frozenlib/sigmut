# Signal の discard 後再購読

- 作成日: 2026-07-22
- 状態: 実装完了

## 目的

`Signal::new` のキャッシュが subscriber 不在時に discard された後、同じ signal を再購読しても値を安全に再計算できるようにする。

## 実装方針

1. UI 非依存の最小回帰テストで、read、discard dispatch、再 read を再現する。
2. discard callback による state 変更と `SourceBinder` の dirty 状態を一体として扱い、再購読時に scan が必ず再計算される契約へ修正する。
3. `on_discard` の callback と `keep` の既存意味を維持する。

## 検証

- 修正前に回帰テストが `get.rs` の `unwrap(None)` で失敗することを確認した。
- 修正後に回帰テストと signal builder 関連テストが成功した。
- format、Clippy、全 target テスト、doc test、rustdoc が成功した。
- coverage で `get.rs` が 100% かつ `scan.rs` の discard 本体が実行されることを確認した。

## 完了条件

- discard 後の再 read で値が再計算され、panic しない。
- `on_discard` と `keep` の既存テストが成功する。
- 本計画書を `docs/plan_archive` へ移し、変更をコミットする。

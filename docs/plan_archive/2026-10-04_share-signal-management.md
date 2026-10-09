# SourceBindings / ScanNode 共通管理の共有化（完了）

## 比較条件（実装前に固定）

- sigmut baseline: `2ba5bf333c0b90d02f2be147d9fd47182f5430ab`。開始時の作業ツリーは clean。
- 作業 branch: `codex/share-signal-management`。
- consumer: acoui `b27afca1b5861405fc4edc952a4c8c626c119418` の既存固定 snapshot を `target/signal-management-probe/source` へコピー。
- rt-local: consumer lockfile の `01bcbd6b6389408db5755af4398d03133a48a88b` を維持。
- Windows MSVC rustc 1.99.0 (`b940084d7`)、LLVM 23.1.1、dev line-tables-only、incremental、既定 256 CGU、jobs=2。
- `cargo test -p acoui-widgets --lib --no-run --locked --offline --jobs 2`。全 workspace feature union の旧調査とは直接比較しない。
- 同じ fixture path / Cargo.lock / features で sigmut のみ Cargo patch で差し替える。before/after は別 target directory。
- compiler mono stats、CodeView procedure code bytes、process private bytes peak を別の指標として記録する。MIR estimate / object size を RAM と解釈しない。

## 実装・検証

1. SourceBindings の準備、context、正常終了処理を通常関数へ移す。panic 時は従来どおり全依存を遅延解除する。
2. ScanNode の sinks / discard scheduling を state / callback と独立した helper にまとめる。data の RefCell は分割せず、再入制約と keep のゼロサイズ scheduling を維持する。
3. 依存の再利用、reset=false、panic / drop、通知時の借用解放、discard の順序を検証する。
4. workspace nextest、doctest、format、Clippy、利用可能な nightly Miri を実行する。
5. widgets test exe の before/after を測定し、結果と再実行方法を記録する。

acoui / rt-local 本体と元の測定資料は変更しない。remote push / main 統合は行わない。

## 完了

実装、回帰テスト、workspace nextest / doctest / Clippy / Miri、consumer 654 テスト、
widgets test exe の各 3 回の before/after 測定を完了した。
生成コードの削減は再現したが、private bytes peak の削減は確認できなかった。
測定用コードと測定報告は削除済み。

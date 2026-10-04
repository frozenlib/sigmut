# SourceBindings / ScanNode の共通管理処理

## 実装

`SourceBindings::update` の sink 作成、`SignalContext` 作成、依存の移動・復元、
不要な依存の解除を既存 `Sink` の通常メソッドに移した。generic wrapper に残るのは
callback 呼び出し、戻り値の保持、準備・完了メソッドの呼び出しだけ。

同期 `ScanNode` では `ScanBindings` が sinks の borrow、bind / unbind / rebind、
dirty 判定・更新、下流への通知を共有する。discard の予約判定と実行直前の
再判定は scheduling cell の型だけに依存し、`()` と `Cell<bool>` の範囲に限定される。
`ScanNodeData::update` は `St` / `Scan` にだけ依存し、`D` / `M` を持ち込まない。
state の更新、callback 呼び出し、map と `StateRef` の射影、discard callback、
具体的な node の Rc 操作は引き続き型に依存する。

新しい Box、heap allocation、callback の動的ディスパッチ、unsafe、状態フラグはない。
`data: RefCell<ScanNodeData<St, Scan>>` と keep のゼロサイズ scheduling cell は維持した。
`ScanBindings` は既存の sinks フィールドだけを包み、scheduling cell は元の node に残す。
実行時間・allocator 呼び出し回数のベンチマークは行っていないため、速度改善は主張しない。

## 維持した契約

- `reset=false` の依存保持、同じ順序の依存の再利用、不要な末尾依存の同期解除。
- callback panic 時は更新先が空のままとなり、移動済みの依存と新たな依存の両方を遅延解除する。
- 正常な callback の後に unbind が panic した場合、callback の戻り値を sink より先に drop する。
  このため完了 helper は sink を所有せず借用する。
- 公開 lifetime と callback の型制約は変更していない。
- data の単一 RefCell による循環依存・再入制約、下流通知の前に data の借用を解放する順序。
- `Scan::FILTER` の MaybeDirty 伝播、重複通知の抑制、更新前の discard scheduling。
- bind / unbind / rebind 後の予約判定、discard 実行時の予約解除・購読有無の再判定、
  discard callback → source clear の順序。

## 測定条件

- baseline: sigmut `2ba5bf333c0b90d02f2be147d9fd47182f5430ab`。
- consumer: acoui `b27afca1b5861405fc4edc952a4c8c626c119418` の固定 snapshot。
- rt-local: `01bcbd6b6389408db5755af4398d03133a48a88b`。
- rustc 1.99.0 (`b940084d7`, LLVM 23.1.1)、Windows x86_64 MSVC。
- `cargo test -p acoui-widgets --lib --no-run --locked --offline --jobs 2`。
- opt-level=0、`debug=line-tables-only`、incremental 有効、既定 256 CGU。
- 同じ source path / Cargo.lock / features / profile。独立 fixture 内の Cargo patch で
  sigmut と sigmut-macros を path package に置換した。lockfile の変更はこの 2 package の
  source 属性だけ。consumer の 2,406 ファイルを照合し、差分は Cargo.toml と Cargo.lock のみ。
- before / after 各 3 回。繰り返しは widgets を `cargo clean -p acoui-widgets` で削除し、
  incremental session が残っていないことを確認してから再生成。依存は再利用した。
  初回の依存を含むビルド時間と反復時間を直接比較しない。
- consumer workspace rustc に測定限定の `-Zdump-mono-stats` / `-Ztime-passes`。
  `RUSTC_BOOTSTRAP=1` は測定プロセス内のみ。
- private bytes は実 rustc プロセスを 20 ms 間隔で採取したピーク。
  link.exe は別プロセスとして扱う。Cargo/rustc/link の合計は約 2～3 秒間隔の補助測定。
- コード量は widgets の新規 incremental objects にある CodeView procedure length の合計。
  共通 helper は sigmut dependency の最新 completed incremental session から別途数える。
  古い incremental session を二重計上しない。

再実行方法は [probe README](../../tools/signal-management-probe/README.md)、
数値・ソース hash・各回の条件は [JSON](2026-10-04_signal-management.json) に保存する。
生データは `target/signal-management-probe`。`before` / `before-2` / `before-3` と
`final-1` / `final-2` / `final-3` が最終比較で、`after` / `after-final` の初期試行は集計対象外。

## 結果

### 生成コード・具体化

以下の procedure code bytes は widgets test の objects 内の集計。各条件の 3 回で一致した。

| 対象 | before | after | 変化 |
|---|---:|---:|---:|
| SourceBindings::update の code bytes | 2,033,240 | 896,491 | −55.91% |
| scan module の code bytes（builder / 新しい ScanNodeData を含む） | 3,207,638 | 2,561,235 | −20.15% |
| sigmut 名前空間の code bytes | 8,990,325 | 7,207,173 | −19.83% |
| widgets objects の全 procedure code bytes | 56,117,955 | 54,334,803 | −3.18% |
| SourceBindings::update の compiler 具体化数 | 2,102 | 2,102 | 不変 |
| ScanNode の update / notify / rebind / discard の compiler 具体化数（各） | 840 | 840 | 不変 |
| widgets 全体の compiler 具体化数 | 285,124 | 285,964 | +840 |
| widgets 全体の MIR estimate | 4,180,614 | 4,066,716 | −113,898 |

SourceBindings の wrapper は 2,102 個残るが、MIR estimate は 1 個あたり 65 → 26。
共有した `Sink::for_update` / `sc` / `finish_update` は sigmut dependency に各 1 procedure、
計 638 code bytes。ScanBindings の共通メソッドも dependency 側にあり、今回採取した
8 メソッドは各 1 procedure、計 1,285 code bytes。これら計 1,923 bytes は widgets の
表には含めず、JSON の `shared_helpers_in_sigmut` に別記した。

総具体化数の +840 は `ScanNodeData::update` の追加分。この consumer では St / Scan の
組も 840 通りあるため、型引数を限定してもこの関数の具体化数は減らなかった。
callback / state に依存する処理は残し、各 node で繰り返されていた sinks の借用管理等を
共有することで、追加 wrapper を含む code bytes は減った。
単に関数数や型名が減ったという結果ではない。

### メモリ・時間

| widgets rustc | before | after |
|---|---:|---:|
| private bytes peak・3 回中央値 | 4,919,103,488 | 4,918,169,600 |
| 同・GiB | 4.58127 | 4.58040 |
| peak の範囲・GiB | 4.57580–4.58522 | 4.57821–4.59893 |
| exe 生成までの compiler 時間・中央値（秒） | 105.729 | 108.624 |
| 同・範囲（秒） | 78.160–106.458 | 77.868–117.428 |

**この測定ではメモリ削減も時間短縮も確認できなかった。** private bytes 中央値の差は
約 0.89 MiB（−0.019%）で、各回のばらつきより小さい。コード生成量の削減は再現したが、
これを RAM の削減率と読み替えることはできない。他のビルドも稼働する環境であり、
対象プロセスの private bytes は分離して採取したが、時間には競合や OS cache の影響がある。

補助的な 2～3 秒間隔の link.exe peak は before 820,645,888–960,901,120 bytes、
after 961,986,560–1,445,720,064 bytes。リンカーの改善も確認できず、短い工程の粗い
サンプリング値として各回の build 合計 peak とともに JSON に残した。このサンプリングの
build 合計 peak は、20 ms 間隔の compiler 単体 peak より低く出ることがあり、
同一時刻の厳密な最大値ではない。

対象は固定 consumer の widgets test、unoptimized / line-tables-only / 診断ありのビルド。
元の全 workspace 調査、release、通常 lib、実行時性能への削減率の外挿はしない。

## 検証

- workspace nextest: 321 passed、既存 ignored compile-fail test 1 件を除外。
- doctest: 11 passed。
- Clippy: `--all-features --tests --lib -- -W clippy::all`、`RUSTFLAGS=-D warnings` 成功。
- format / diff check 成功。
- nightly Miri: library 308、integration 7 passed。CI と同じく should-panic 6 件を除外、
  ignored compile-fail test 1 件は未実行。
- 新しい SourceBindings の 4 ケースは baseline 実装でも実行してすべて成功。
  通知の再借用・slot / FILTER / 重複抑制・keep の discard 非予約を 1 ケースで確認した。
- 最終実装をリンクした acoui-widgets テスト exe: 654 passed。

acoui / rt-local 本体、元の測定資料、公開 API、依存 version は変更していない。
remote push / main 統合は行っていない。

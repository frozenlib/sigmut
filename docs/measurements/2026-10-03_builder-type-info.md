# Builder の型情報削減の測定

2026-10-03 に、baseline `92c4d7331464c8818fbfbf750f751ecc19b22d04`
と改善後の作業ツリーを比較した。比較用コードと実行方法は
[type-info-probe](../../tools/type-info-probe/README.md)、各回の数値と rustc の版は
[JSON](2026-10-03_builder-type-info.json) に記録する。

## 変更

- `Build::map`、`SignalBuilder::map_value`、`flat_map` のアダプタを、入力値型・出力値型・関数型だけを持つ local helper で生成する。`map_borrow` は Borrow の関数 item を使う。
- get と dedup の scan を名前付きコールバックにし、Option の discard / 射影は getter 型を持たない内部型にする。
- 同期・非同期 scan の discard を、既存 Rc を unsize した `Rc<dyn DiscardReaction>` で実行する。discard 専用の Fn アダプタと Any downcast を取り除く。

公開 API、FILTER、keep の ScheduledCell、StateRef による所有者保持、
discard の scheduling 判定と実行時の再判定を維持した。
map / scan 処理の動的ディスパッチや heap allocation を追加しない構造であり、
discard は既存の動的呼び出しを専用 trait 呼び出しに置き換える。
実行時の速度・割り当て回数のベンチマークは行っていない。

## 条件

- rustc 1.99.0 (`b940084d7`, LLVM 23.1.1)、x86_64-pc-windows-msvc。
- opt-level=0、codegen-units=16、incremental 無効。
- get と scan をそれぞれ 4 回具体化。参照 map / 値 map を交互に 0、3、6 段重ねる。
- 依存のビルドは測定外。同じ Cargo.lock と設定を使用する。
- 実際の rustc を直列起動し、各回の出力先を新規作成。各条件 3 回。
- private bytes は compiler 本体を 20 ms 間隔で採取したピークの中央値。依存、linker、全ビルドの合計ではない。
- 型名は rlib 中の CodeView LF_STRUCTURE の主名称を数える。最終 PDB や全種類の型レコードは測定していない。

初回の試行はメモリ採取が rustup proxy を対象にしていたため、メモリ結果を破棄した。
以下は compiler 本体を直接採取した再実行の結果である。

## debuginfo=2 の結果

サイズは MiB。ファイルサイズと型名の長さは 3 回とも一致した。

| map 段数 | 最長型名: 前 → 後（文字） | .debug$T: 前 → 後 | .debug$S: 前 → 後 | rlib: 前 → 後 | rustc private peak 中央値: 前 → 後 |
|---|---:|---:|---:|---:|---:|
| 0 | 2,520 → 443 | 2.357 → 1.328 | 1.841 → 0.968 | 9.468 → 5.874 | 205.195 → 206.680 |
| 3 | 12,238 → 888 | 9.740 → 2.283 | 8.328 → 1.813 | 32.626 → 10.069 | 275.105 → 221.668 |
| 6 | 49,175 → 1,340 | 70.367 → 3.447 | 60.928 → 2.818 | 212.977 → 14.506 | 689.398 → 246.203 |

6 段では最長型名が約 97.3%、.debug$T が約 95.1%、rlib が約 93.2%、
private peak の中央値が約 64.3% 減少した。
private peak の範囲は変更前 684.7–695.6 MiB、変更後 237.5–248.2 MiB。
0 段のメモリについては改善を確認できず、ほぼ同じだった。

6 段の LF_STRUCTURE は、異なる名称数が 1,165 → 1,110、レコード数が
9,600 → 8,720、繰り返しを含む型名総バイトが 64.392 → 2.421 MiB。
この例では型の種類の削減よりも、各名前の再帰的な膨張を抑える効果が大きい。

コンパイル時間の中央値は 6 段で 1.194 → 0.395 秒。
短い測定で OS cache や実行順の影響があるため、時間は参考値とする。

## line-tables-only の対照（6 段）

| 指標 | 変更前 | 変更後 |
|---|---:|---:|
| .debug$T（MiB） | 0.019 | 0.018 |
| .debug$S（MiB） | 17.287 | 0.921 |
| rlib（MiB） | 46.351 | 6.047 |
| rustc private peak 中央値（MiB） | 332.742 | 195.289 |

LF_STRUCTURE は両方とも 0 件。この条件でも private peak は約 41.3% 減った。
型情報セクションの容量と RAM 使用量を同一視できない。
line-tables-only でもジェネリック由来の関数名等は残るが、今回の測定は
compiler 内のメモリを用途別に分解しておらず、削減の内訳までは断定しない。

## 解釈と制約

これは不要な builder 型の再混入を強調する人工的な例である。
acoui の多数の値型、std の型、CGU 間重複、他の generic code を含む
core 全体に、同じ削減率が出るという結果ではない。
3 種類の変更をまとめて比較しており、個別の寄与は分離していない。
最適化ビルド、最終 PDB、実行時の性能、acoui 本体は未測定。

## 検証

- workspace nextest: 316 件成功、既存の ignored compile-fail test 1 件は除外。
- doctest: 11 件成功。
- Clippy: `--all-features --tests --lib`、`-D warnings` / `-W clippy::all` 成功。
- format check / diff check 成功。
- nightly Miri: library 303 件と integration 7 件が成功。should-panic 6 件は CI の指定どおり除外。
- 新しいテストは discard の再購読・再計算、unsized な map 連鎖、既存 Rc の共有と解放を確認する。

# Builder type information probe

Windows / MSVC 向けの小さな downstream crate を直接 rustc でコンパイルする。
`fixture.rs` は get と scan の両方で、参照 map と値 map を交互に重ねる。
各経路を 4 回モノモーフィック化し、外側の builder 型を引き継ぐ
クロージャによる型名の増加を意図的に発生させる。

```powershell
python tools/type-info-probe/measure.py --baseline 92c4d733 --depths 0 3 6 --repeat 3
```

Python の標準ライブラリだけを使う。Git、Cargo、rustup、rustc と、
`--offline --locked` で依存をビルドできる Cargo cache が必要。
既存の source や target は上書きしない。出力は実行ごとに新しい
`target/type-info-probe/<UTC timestamp>/` に保存する。

比較条件:

- baseline は `git archive` で読み取り専用の比較用 source として展開する。
- baseline と作業ツリーに同じ作業ツリーの Cargo.lock を使う。
- 各条件の依存ビルドを先に完了し、その時間・メモリを測定から除く。
- `rustup which rustc` で実際の compiler を選び、downstream rustc を直列実行する。
- opt-level=0、codegen-units=16、incremental 無効。依存の Cargo jobs=1。
- 全 depth を debuginfo=2、最大 depth を line-tables-only で比較する。
- 各 downstream コンパイルは新しい出力ディレクトリを使う。

`summary.json` は rustc の版、ライブラリの差分、コマンド、各回の結果を含む。
private bytes は compiler 本体の PrivateUsage を 20 ms 間隔で採取した最大値。
短いピークを取り逃がし得るため、OS の保証された最大値ではない。
ビルド全体、依存 compiler、linker のメモリは含めない。

COFF の `.debug$T` / `.debug$S` と rlib 全体のサイズを別々に集計する。
型名集計は LF_STRUCTURE の主名称のみであり、全 CodeView レコードや
最終 PDB の重複除去結果を表さない。最長名は CodeView 表現での文字数。
`type_name` を使うとその文字列自体がバイナリに追加されるため、使わない。

この例は不要な builder 型の再混入を強調する。acoui 全体の削減率を
予測するための例ではない。実行時の速度・割り当て数も測定していない。

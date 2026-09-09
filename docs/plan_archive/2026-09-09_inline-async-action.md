# インライン非同期 Action

## 目的と契約

- `ActionContext::start_async_inline` / `start_async_inline_in` を追加する。
- 所有する `AsyncActionContext` を受け取る `AsyncFnOnce` を、その context で同期的に一度 poll する。Ready なら return 前に完了・future 解放、Pending なら runtime が所有して wake 後に継続する。
- `resume_phase` は継続先のみを指定し、開始前に RuntimeConfig で検証する。既定版は既定 phase を使う。
- `AsyncActionContext::is_running_inline` はインライン開始メソッド内の最初の poll 中のみ true。即座に完了する await では true を維持し、再開時・予約開始時・poll 外では false。
- 既存 spawn 系の予約開始と借用 context のシグネチャを維持する。
- 新規 unsafe、Send / Sync 制約、公開 context constructor、別 executor は追加しない。acoui と依存 revision は変更しない。

## 実装方針

- private `AsyncAction::start`、context scope、wake 配送、runtime の所有・取消処理を共通利用する。
- context source と context で channel および実行中フラグを共有し、フラグを panic 時も復元する。
- フラグは poll 中に借用される future の RefCell から独立させる。
- 既存の ActionContext の crate root re-export を利用し、メソッドの公開ドキュメントと使用例を追加する。

## 検証一覧

- [x] Ready と Ready な await 後の状態更新・capture 解放が return 前に完了。
- [x] Pending 後は呼び出し元が進み、wake 後は指定 phase でのみ継続。以後フラグは false。
- [x] self-wake しても開始呼び出し中は一度しか poll しない。別の queued Action を dispatch しない。
- [x] runtime 終了で future・capture を解放し、完了・取消後の wake で handler が復活しない。
- [x] 無効な明示・既定 phase を handler 実行前に拒否。
- [x] spawn 系は予約 dispatch まで実行せず、常にフラグ false。
- [x] 複数 invocation、ネストした開始、poll 外および panic 後のフラグの独立性。
- [x] 関連テスト、全体テスト、Clippy 自動修正、doc tests、rustdoc、追加部分のカバレッジ確認。

## 進捗

- 2026-09-09: 実装・検証完了。本書を `docs/plan_archive` に移動。
- 共通の開始処理に phase 検証と初回 poll の inline 指定を追加。継続 poll・wake 配送・取消は既存経路を利用。
- フラグは実際の poll の周囲だけで設定し、Drop guard により Ready・Pending・panic のいずれでも false に戻す。
- 公開メソッドと既存 spawn 系の doc comment を整備し、状態変更の実行可能な例を追加。crate root の既存 re-export から利用可能。

## 検証結果

- `cargo nextest run -p sigmut async_action`: 16 件成功（新規 15 件、既存 1 件）。
- `cargo fmt`: 成功。
- `cargo clippy --fix --allow-dirty --allow-staged --all-targets`: 成功。Clippy 診断なし。
- `cargo nextest run --workspace --all-targets`: 313 件成功、既存の ignored テスト 1 件スキップ。
- `cargo test --doc --workspace`: 11 件成功。
- `cargo doc --no-deps --workspace`: 成功。rustdoc 診断なし。
- ビルド時には Windows linker のライブラリ生成メッセージが `linker_messages` 警告として出力される。
- `cargo llvm-cov --lib --json --output-path target/llvm-cov/coverage.json -- async_action`: 計測下でも 16 件成功。初回は出力先ディレクトリが未作成だったためレポート保存のみ失敗し、ディレクトリ作成後に以下で既存計測結果から出力した。
  - `cargo llvm-cov report --json --output-path target/llvm-cov/coverage.json`
  - `cargo llvm-cov report --text --show-missing-lines --output-path target/llvm-cov/coverage.txt`
- 追加した公開 3 メソッド、共有フラグの設定・解除、共通 poll 経路の追加行に未実行行なし。Ready / Pending、再開、panic 後の復元を確認し、仕様のテスト漏れなしと判断。
- `git diff --check`: 成功。

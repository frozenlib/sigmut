# Builder の型情報削減

完了。測定条件、結果、検証は [報告書](../measurements/2026-10-03_builder-type-info.md) を参照。

- map アダプタを値型と関数型だけを持つ helper で生成する。
- get の scan / discard / Option 射影を名前付き内部コールバックにする。
- 同期・非同期 scan の discard を既存 Rc の専用 trait object で実行する。
- 公開 API、通知、キャッシュ、discard / keep の意味を維持する。
- 意図的に長い map 連鎖を生成する downstream fixture と比較スクリプトを残す。
- 同じ rustc、debuginfo、codegen-units、jobs で変更前後の型情報とメモリを別々に測定する。
- 関連テスト、format、Clippy を実行し、測定結果と制約を記録する。

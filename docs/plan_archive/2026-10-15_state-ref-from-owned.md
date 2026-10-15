# `StateRef::from_owned` の追加（完了）

## 実装方針

- `Borrow<T> + 'static` の所有値から `StateRef<T>` を作れる公開メソッドを追加する。
- `StateRef::from_value` と `StateRef::map` を組み合わせ、既存の Bump allocation と所有元保持を再利用する。
- 非 `'static` な所有値や `StateRefBuilder` の API は今回の対象外とする。
- 所有元が `StateRef` の破棄まで保持されることをテストする。

## 完了

- `StateRef::from_owned` と API ドキュメントを追加した。
- `String` から `StateRef<str>` を作成できることを確認した。
- 所有元の保持と破棄を回帰テストで確認した。
- `cargo fmt --all -- --check`、`cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo doc --no-deps -p sigmut` が成功した。

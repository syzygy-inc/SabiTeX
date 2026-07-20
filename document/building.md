# ビルドとテスト

## 前提

- Rust(stable)。
  `rust-toolchain.toml` により、rustup が stable と rustfmt、clippy、wasm32 ターゲットを自動で用意する。
- 必須の外部依存はない。
  TeX Live は任意である(インストールされていれば、CLI が TFM や .sty をそこから解決する。[cli.md](cli.md) を参照)。

## ビルド

```
cargo build --release -p sabitex-cli        # ネイティブ CLI (バイナリ名: sabitex)
cargo build --release -p sabitex-wasm --target wasm32-unknown-unknown
```

wasm 成果物は `target/wasm32-unknown-unknown/release/sabitex_wasm.wasm` に置かれる。
使い方は [wasm.md](wasm.md) を参照。

OpenType シェーピング(XeTeX 層の native font)は feature `shaping`(rustybuzz)が担う。
`sabitex-cli` は既定で有効にする。
`sabitex-wasm` で必要な場合は `--features shaping` を付ける。

## テスト

```
cargo test --workspace                     # 全テスト
cargo test -p sabitex-core --test trip      # TRIP (Knuth の適合性テスト)
cargo test -p sabitex-core --test etrip     # e-TRIP (e-TeX の適合性テスト)
```

TRIP と e-TRIP の入力と参照データは `reference/tex/trip/` と `reference/etex/etrip/` に vendoring 済みで、ネットワークも TeX Live もなしで走る。
比較規則は `specification/trip.md` と `specification/etrip.md` を参照。

CI(`.github/workflows/ci.yml`)は `cargo fmt --check`、`cargo clippy -D warnings`、全テスト、wasm32 ビルドを検証する。

[English](README.md) | **日本語**

# SabiTeX

tex.web(Knuth の TeX82)を一次仕様とする、TeX 処理系の Rust 再実装である。
TeX82 に e-TeX、XeTeX 拡張、pTeX/upTeX 互換の和文組版(npTeX 方式)を加えて単一エンジンに実装し、DVI / XDV を出力する。

名前は Rust の和訳「錆び」と、侘寂の「寂び」に由来する。
読みは「さびてふ(寂びTeX)」または「さびてく(錆び/寂びてく)」。
枯れて苔生すまで保守を続けるという意思を込めている。

- **TRIP / e-TRIP テスト準拠**：Knuth の TRIP と e-TeX の e-TRIP を INITEX / VIRTEX の両パスで再現する。
  比較規則は `specification/trip.md` と `specification/etrip.md` に記す。
- **依存ゼロのコア**：`sabitex-core` は外部依存を持たず、wasm32 でそのまま動く。
  OpenType シェーピングだけが optional な rustybuzz に依る。
- **和文組版**：JFM による組版、\kanjiskip と \xkanjiskip、禁則を pTeX/upTeX 準拠でエンジンレベルに実装する(`specification/japanese.md`)。

## クイックスタート

```
cargo build --release -p sabitex-cli
cd examples/hello
../../target/release/sabitex hello     # → hello.dvi (XDV)
```

TeX Live なしで動く自己完結サンプルは [examples/](examples/README.md) を参照。
CLI の詳細は [document/cli.md](document/cli.md) に、ブラウザや wasm での利用は [document/wasm.md](document/wasm.md) に記す。

## リポジトリ構成

| パス | 内容 |
|---|---|
| `crates/sabitex-core` | エンジン本体(依存ゼロ、I/O は trait 経由) |
| `crates/sabitex-cli` | ネイティブ CLI(バイナリ名 `sabitex`) |
| `crates/sabitex-wasm` | wasm バインディング(手書き C ABI) |
| `examples/` | 主張 1 つずつのサンプル(欧文 OpenType、和文 JFM、和欧混植、TFM 数式、wasm ABI) |
| `document/` | 使い方(ビルド、CLI、wasm) |
| `specification/` | 設計判断と挙動仕様、テスト比較規則の記録 |
| `reference/` | 第三者由来の一次仕様とテストデータ(ソフトウェア別。各ファイルのライセンスに従う) |

## ビルドとテスト

```
cargo test --workspace          # TRIP / e-TRIP を含む全テスト
```

詳細は [document/building.md](document/building.md) を参照。

## ライセンス

本プロジェクトのコードのライセンスは `LICENSE` を参照。
`reference/` 配下の第三者由来ファイルは含まれず、それぞれ元のライセンスに従う(`reference/README.md`)。

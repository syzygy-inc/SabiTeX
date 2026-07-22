# サンプル

各サンプルは異なる主張を 1 つだけ持つ。
いずれもフォーマット(plain 等)を使わず、INITEX から直接組む。
各サンプルのディレクトリに入り、事前にビルドした CLI で実行する。

```
cargo build --release -p sabitex-cli
```

| サンプル | 主張 | 実行(各ディレクトリ内で) | 依存 |
|---|---|---|---|
| `hello/` | OpenType native font(XeTeX 拡張)で XDV を出力できる | `../../target/release/sabitex hello` | なし |
| `japanese/` | JFM による和文組版(upTeX 方式の set2/set3 出力) | `../../target/release/sabitex japanese` | umin10.tfm(下記) |
| `mixed/` | 和欧混植の段落。xkanjiskip の和欧間空きと禁則が効く | `../../target/release/sabitex "*\input mixed"` | umin10.tfm(下記) |
| `math/` | TFM ベースの数式(Computer Modern) | `../../target/release/sabitex math` | TeX Live |
| `wasm-node/` | wasm ABI を node から実行し、missing-file プロトコルを実演 | `node main.mjs` | node + wasm ビルド |

## フォントメトリックの解決(japanese / mixed)

これらは和文メトリック `umin10.tfm` を名前で参照する。
TeX Live があれば kpsewhich 経由で自動解決される。
ない環境では、`reference/uptex/umin10.tfm` をサンプルのディレクトリへコピーすれば動く(CLI はカレントディレクトリを最初に探す)。
パスで `\font` 指定しないのは、DVI のフォント名にパスが焼き込まれ、xdvipdfmx が `kanjix.map` を引けなくなるためである。

## 拡張モード(mixed)

和文パラメータ(kanjiskip、xkanjiskip、禁則ペナルティ)のプリミティブは、互換モードを TeX82 と区別不能に保つため拡張モード限定である。
e-TeX 系の慣例どおり、`**` 行を `*` で始めると拡張モードになる(表の `"*\input mixed"` がそれ)。

## wasm-node の準備

wasm を事前にビルドしてから実行する。

```
cargo build --release -p sabitex-wasm --target wasm32-unknown-unknown
cd examples/wasm-node
node main.mjs    #=> doc.dvi
```

1 パス目は VFS に入力だけを入れて実行し、`<missing>` に不足ファイル(cmr10.tfm)が列挙される。
2 パス目でリポジトリ内の TFM を追加して再実行し、DVI を得る。

## PDF への変換

DVI / XDV は `xdvipdfmx`(TeX Live)で PDF にできる。

```
xdvipdfmx hello.dvi
xdvipdfmx japanese.dvi    # umin10 の実フォント解決に kanjix.map を使う
```

和文の DVI は JFM がメトリック専用のため、グリフの実体は xdvipdfmx が `kanjix.map`(例：umin10 → 原ノ味明朝)で解決する(`specification/japanese.md` を参照)。

## CI

hello、japanese、mixed、wasm-node は CI でもスモーク実行される。
math は TeX Live 依存のため CI の対象外である。

## 補足

- フォーマット(plain 等)を使う例は `document/cli.md` を参照。
- wasm ABI の詳細は `document/wasm.md` を参照。
- 各 `.tex` 先頭の `\catcode` 行は、INITEX で `{` `}` 等に構文上の役割を与える定型である(フォーマットを使えば不要)。

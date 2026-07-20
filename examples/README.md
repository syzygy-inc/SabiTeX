# サンプル

どちらもフォーマット不要(INITEX)で動く。
各サンプルのディレクトリから実行する。

```
cargo build --release -p sabitex-cli

cd examples/hello
../../target/release/sabitex hello
#=> hello.dvi (XDV: OpenType native font を使うため id_byte=7)

cd ../japanese
../../target/release/sabitex japanese
#=> japanese.dvi (pDVI 相当: JFM + set2/set3、id_byte=2)
```

| サンプル | 内容 |
|---|---|
| `hello/` | XeTeX 拡張の `\font\x="[パス]"` で OpenType フォント(Latin Modern)をリポジトリ内から直接読み、1 行組む。TeX Live 不要 |
| `japanese/` | JFM(umin10)による和文組版。和文文字は upTeX 方式で set2/set3(USV)出力 |

## フォントメトリックの解決(japanese)

`japanese.tex` は和文メトリック `umin10.tfm` を名前で参照する。
TeX Live があれば kpsewhich 経由で自動解決される。
ない環境では、`reference/uptex/umin10.tfm` をこのディレクトリへコピーすれば動く(CLI はカレントディレクトリを最初に探す)。
パスで `\font` 指定しないのは、DVI のフォント名にパスが焼き込まれ、xdvipdfmx が `kanjix.map` を引けなくなるためである。

## PDF への変換

出力は `xdvipdfmx`(TeX Live)で PDF にできる。

```
xdvipdfmx hello.dvi
xdvipdfmx japanese.dvi    # umin10 の実フォント解決に kanjix.map を使う
```

`japanese.dvi` は JFM がメトリック専用のため、グリフの実体は xdvipdfmx が `kanjix.map`(例：umin10 → 原ノ味明朝)で解決する(`specification/japanese.md` を参照)。

## 補足

- フォーマット(plain 等)を使う例は `document/cli.md` を参照。
- wasm から同じ入力をコンパイルする例は `document/wasm.md` を参照。
- `.tex` 先頭の `\catcode` 2 行は、INITEX で `{` と `}` をグループ文字にする定型である(フォーマットを使えば不要)。

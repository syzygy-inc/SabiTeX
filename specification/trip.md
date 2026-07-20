# TRIP テストの手順と比較規則

Knuth の TRIP テストを INITEX / VIRTEX 両パスで再現し、参照ログとの一致を検証する。
trip.dvi は、dvitype 出力が Knuth の trip.typ とバナー行を除き完全一致(2920 bytes / 16 pages)であることを要求する。

参照データは `reference/tex/trip/` に置く(CTAN systems/knuth/dist/tex から vendoring。trip.tfm は `pltotf trip.pl trip.tfm` で生成。`tripman.tex` が Knuth 自身による手順書)。

## 手順(tripman.tex §1-§13)

1. `tftopl trip.tfm` が `trip.pl` と一致することを確認する(フォント側の前提)。
2. INITEX パス：TRIP パラメータの INITEX で `\input trip` を実行し、`trip.fmt` を dump する。
   転写を `tripin.log` と比較する。
3. VIRTEX パス：`&trip trip` で再実行する。
   転写を `trip.log` と、端末出力を `trip.fot` と比較する。
4. `dvitype trip.dvi`(オプション：output level 2, resolution 72.27, page `*.*.*.*.*.*.*.*.*.*`)を `trip.typ` と比較する。

エンジンパラメータは `tests/trip.rs` の `trip_sizes()` に定める。
mem_bot=mem_min=1(tripman 指定。`still untouched` と `words of memory out of 3000` がこの値で初めて一致する)、mem_top=3000、error_line=64、half_error_line=32、max_print_line=72、font_max=75 で、ほかは tex.web §11-§12 の既定値。

VIRTEX パスは `run_terminal_job()` で実行する(§37+§1332+§1337 忠実：端末プロンプトと echo、`&format` スキップ、escape でなければ \everyjob より前に start_input)。

## ハーネス

`crates/sabitex-core/tests/trip.rs`。
実行：

```
cargo test -p sabitex-core --test trip            # INITEX + VIRTEX 両パス
```

成果物は `target/trip/ours-trip{in,}.{log,fot,dvi}`。
dvitype 比較：

```
dvitype -output-level=2 -dpi=72.27 -page-start='*.*.*.*.*.*.*.*.*.*' \
    target/trip/ours-trip.dvi | diff --strip-trailing-cr - reference/tex/trip/trip.typ
```

差分はバナー行 1 行のみであること(テストは byte 数 2920 を検証する)。

## 比較規則と根拠

本エンジンは Unicode(XeTeX 意味論)の上で TRIP を通すため、参照ログとの差分のうち以下だけを正当なものとして許容する。
それ以外は 1 行単位で完全一致を要求する(行分割とハイフネーションのトレース、\show 系、trie 統計、current usage を含む)。

- **正当差分ブロック**(`LEGIT_REF_BLOCKS`)：`\lccode256` が有効(Unicode/XeTeX 意味論)のため、「! Bad character code (256).」の 8 行ブロックが発生しない。
  参照側から除去して比較する。
  同様に、数式族が 256 個(xetex.web `scan_math_fam_int`)のため、`\textfont16=` に対する §577 の「! Bad number (16).」8 行ブロックも発生しない(続く `=\relax` への「! Missing font identifier.」は双方で発生し一致する)。
- **escapechar=256**(VIRTEX)：TeX82 の print_esc は 0..255 のみ印字するが、本エンジンは XeTeX 同様 U+0100 を印字する。
  比較時に本エンジン側から U+0100 を除去し、幅起因の折返しとクロップの差は `LEGIT_PAIRS` で対にして許容する。
  空白のみのパディング行は幅差を許容する。
- **マスク行**：バナー、`**` 行、strings 統計(TEX.POOL 非搭載と遅延 intern で文字列数が異なる)、memory locations dumped(本ポートは全配列を dump し、§1311 の圧縮語数が無意味)、words of font info。
- **words of memory / Memory usage(still untouched)**：既知の未解決差分。
  l.285 の hairy display 処理中の一時 variable-size 確保が lo-mem リングを参照実装と異なる形に断片化させ、§126 の成長が 1 回余分に起こる(ours 86 / ref 175、±89 words)。
  各 shipout 時点の var_used と dyn_used はすべて一致しており、DVI と組版結果には影響しない。

比較一致のために確定させた実装仕様：

- glue_set は f32 経由で格納する(memword `set_gr`)。
  web2c の `glue_ratio` は単精度であり、f64 のままだと glue set 表示と DVI の glue 丸めが半 ulp ずれる(例：16341.99998 vs 16342.0fil)。
- \special のバイト出力(§1368)：USV が 256 未満なら生 1 バイト(TRIP の `^^80`)、256 以上なら UTF-8。
- §31 input_ln の忠実化：行は生のまま保持し、`copy_line_to_buffer` で trailing blank の除去と `max_buf_stack` の生長更新を行う。
- §1334 は wlog 経由とし、file_offset を更新しない(保存と復元により、「Output written」前の空行という実 TeX の癖を再現する)。
  バナーも wlog 同様に生書きする(折返しなし、offset 不算入)。

## 対象外

trip.fmt 自体のバイト比較は行わない(fmt は独自固定幅 LE 形式という設計判断のため。[architecture.md](architecture.md) を参照)。

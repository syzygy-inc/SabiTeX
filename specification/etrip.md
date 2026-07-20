# e-TRIP テストの手順と比較規則

`crates/sabitex-core/tests/etrip.rs` が、e-TeX の e-TRIP テスト(etex.ch Appendix / `reference/etex/etrip/`)を再現する。
INITEX パス(`*etrip`)と VIRTEX パス(`&etrip etrip`)の両方に対し、以下のマスク済み差分を除いて参照 `etripin.log` / `etrip.log` / `etrip.fot` と一致することを要求する。
さらに `etrip.out` はバイト同一であること、`etrip.dvi`(220 bytes)は dvitype 出力が参照 `etrip.typ` と banner および DVI コメントの日付以外で一致することを要求する。

## 実行条件

- サイズは `reference/etex/etrip/texmf.cnf` と web2c 固定値(mem 0..3999、pool 32000、max_strings 3300、error_line 64/32/72、hash 15000/8501、trie_op_size 35111、hyph_size 659)。
- 標準入力リダイレクト相当(`Engine::terminal_echo = false`)。
  `**` プロンプトへタイプした行はエコーされない。
- `Mem::glue_ratio_wide = true`。
  TeX Live のバイナリは glue_ratio が double である。
  TRIP(Knuth 参照)は f32 のままなので、既定は false。

## マスク規則

banner、`**` エコー、日付、"memory locations dumped"、"strings of total length"、"words of font info"、"Memory usage"、"words of memory out of" などの容量と統計の行をマスクする。
TRIP と同趣旨である([trip.md](trip.md) を参照)。
加えて次の 4 点を許容する。

1. **multiletter control sequences(407 vs 408)**：TeX Live の e-TeX バイナリは、ソース(tex.web + etex.ch)より multiletter cs を 1 個多く報告する。
   compat モードの trip でも 341(Knuth)に対し 342 で、`reference/etex/etrip/etrip.diffs` 自身が acceptable diff として記録している。
   本ポートはソース準拠の 407 である。
2. **hyphenation exceptions(11 vs 10)**：web2c は再宣言された \hyphenation 例外を hyph_link チェーンで置換するが、tex.web §940 は 2 エントリ目を挿入する。
   etrip は言語 3 の qq-B-pp を再宣言するため、総数が 1 違う。
   lookup が返すハイフン位置は同一である。
3. **`! Bad character code (256).` ブロック(参照のみ)**：本ポートは XeTeX 流の Unicode 幅文字コードを持つため、256 は正当な文字番号であり、§434 相当のエラーが出ない(TRIP の `\lccode256` と同じ設計差)。
4. **`(./etrip.tex` の `./`**：kpathsea のパス解決痕。
   比較時に参照側の `(./` を `(` に、`` `./ `` を `` ` `` に正規化する。

## 確定した挙動(実装が準拠する仕様解釈)

- pseudo file(\scantokens)の `max_buf_stack` は §31 と違い、「4 文字/word にパディングした行長 + 1」で更新される(etex.ch の pseudo_input はコピー後の `last` で `max_buf_stack:=last+1`)。
- `\let` で REGISTER/TOKS_REGISTER cs をコピーするとき、sparse leaf に `add_sa_ref` する(etex.ch §1221)。
  欠けると参照カウントが -1 になり、二重解放に至る。
- `\tracinglostchars>1` の char_warning は、tracingonline を一時 1 にして端末へも出す。
  端末とログの桁位置非対称により、直前に \message があるとログ側に空行が生まれる(§62 print_nl の仕様どおり)。
- normal_paragraph(§1070)は \parshape に加えて \interlinepenalties もリセットする。
- glueexpr の加減算(etex.ch)は、stretch/shrink のオーダーが異なると、減算でも符号を反転せずに新項の成分をコピーする。
- hpack の LR 検査(TeXXeT)は "\endL or \endR problem" 報告後、`goto common_ending` が `exit:` の検査ブロックへ再入して番兵を pop する(報告パスでも pop が要る)。
- 単語スキャン(§894/adv_past)の language_node でも `set_hyph_index` を呼ぶ(\savinghyphcodes の保存コード切替)。
- §283 の restore トレースは、eqtb へ書き戻した後に \tracingrestores を判定する(パラメータ自身を 0 に戻す restore は報告されない)。
- §336 の "Incomplete \if" は、EOF 由来(cur_cs=0)ならヘルプ 1 行目が "The file ended while I was skipping conditional text." に差し替わる。

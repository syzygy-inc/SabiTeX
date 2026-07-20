# 和文組版(npTeX 方式)の設計

一次資料は `reference/ptex/ptex-base.ch`(pTeX 本体、8,181 行)と `reference/uptex/uptex-m.ch`(upTeX = pTeX の Unicode 化、2,732 行)である。
参照実装は euptex(TeX Live)で、挙動の突合せに使う。
方針は北川弘典氏の npTeX 構想(XeTeX に pTeX 相当の和文組版をエンジンレベルで搭載し、和文は JFM 中心とし、OpenType による和文組版はしない)に従う。
実装の中心は `kanji.rs`。

## 和文文字の内部表現

pTeX の **2 連 char_node** 表現を踏襲する。
pTeX の和文文字は、連続する 2 つの char_node である。

```
1 個目: font = 和文フォント (font_dir[f] ≠ dir_default)
        character = JFM の char_type(文字クラス、メトリックの鍵)
2 個目: info = 文字コード(upTeX では USV)
```

表示や走査の系は `font_dir[font(p)] ≠ default` を見て 2 個目を読み飛ばす(ptex-base.ch [12.183] ほか全域)。
この表現を踏襲する理由は次のとおり。

- hpack の JFM グルー挿入、行分割の和文分割点、禁則がすべてこの表現前提で書かれており、忠実移植が最も安全である。
- is_char_node の高速路(hi_mem)を保てる。
- char_node の character フィールドは u16 だが、実コードは 2 個目の info(halfword、USV 全域)に入るので幅の問題がない。

sabitex 固有の注意：`font_dir` は FontMem の並行配列(`dir: Vec<u8>`、0=default/1=yoko/2=tate。現状は yoko のみ)。
和文ノードペアの生成、削除、コピーは必ずヘルパーを経由し、片割れ孤児を作らない。

## JFM(id = 11 横 / 9 縦)

TFM との差(ptex-base.ch [30.560]):

```
先頭に id[2] nt[2] が付く(TFM は lf が先頭)。id=11/9 で JFM と判別。
nt = char_type テーブル(文字コード → 文字クラス)のワード数。
bc=0 必須。lf = 7 + lh + nt + (ec-bc+1) + nw + nh + nd + ni + nl + nk + ne + np。
char_type テーブル: 各ワード (コード, クラス) のペア。クラス 0 が既定。
lig/kern の代わりに glue/kern(gk_tag): クラス間の挿入グルー
(jfm_skip)またはカーン。
```

フォント読み込みは read_font_info の頭で id を見て JFM に分岐する(ctype_base 等のベースを別に持つ)。
pTeX 系の実際は \font がそのまま JFM も読む(id で自動判別)ため、sabitex も \font に統合し、id=11/9 を検出したら font_dir を設定する。

## 自動挿入グルー(euptex 実測で確定した実像)

- \kanjiskip は二層構造である。
  和文文字ペアが直接隣接する場合は暗黙で、リストにノードは無く、hpack の幅と伸縮の勘定、行分割の act_width、hlist_out の送りにだけ効く。
  間に禁則ペナルティ等が挟まる場合は、adjust_hlist(行分割と hbox package の直前に走る後処理)が実グルーノード(subtype 19)を挿入する。
- \xkanjiskip は、直接隣接でも常に実ノードである。
  adjust_hlist が A-K / K-A / 合字境界 / ペナルティ越しの 4 経路で挿入し、\xspcode(欧文側：bit0 = 後ろ可、bit1 = 前可、既定は英数字 3)と \inhibitxspcode(和文側：0=両禁 1=前禁 2=後禁)で抑制する。
- JFM の glue/kern(クラス対で JFM が指定する約物の詰め等)は \kanjiskip より優先し、subtype = jfm_skip (20) のグルーになる。
- 取り込み(latch)は box package と line_break の冒頭で行う。
  \autospacing / \autoxspacing が off なら zero_glue。
- box が測定に使った spec は box_spacing サイドテーブル(Engine の BTreeMap)に持つ。
  pTeX は box_node を広げて space_ptr/xspace_ptr を焼くが、sabitex は box ノードを 7 語のまま保ち、TRIP と e-TRIP のメモリ統計互換を守る。
- 行頭の JFM グルーは adjust_hlist が除去し、行末の JFM グルーは zero spec に差し替える。
- main_loop(欧文高速路)の lookahead で和文文字を検出したら、cur_r=NON_CHAR で合字境界を経由して抜け、reswitch が append_kanji に渡す(uptex-m.ch 準拠)。

## 禁則とペナルティ

- \prebreakpenalty`c=n`(行頭禁則：c の前に penalty n)と \postbreakpenalty`c=n`(行末禁則)。
  pTeX は kinsoku_base の 1024 エントリのハッシュテーブル(eqtb 内、save/restore 対象)。
- 挿入は行分割走査時で、subtype = kinsoku_pena (2) の penalty。
- \jcharwidowpenalty：段落最終行が和文 1 文字で終わるのを防ぐ。

## DVI/XDV 出力

和文文字は pTeX/upTeX 方式で出力する。
すなわち、通常の fnt_def(JFM 名)と set2/set3(USV)である。
id_byte は native font 不使用なら 2(pDVI 相当)。
xdvipdfmx が kanjix.map(例：umin10 → HaranoAjiMincho)で解決して CIDFontType0 埋め込み PDF を生成できる(受け入れ試験：「こんにちは、世界。」→ pdftotext 一致)。
native font(XDV id=7)と混在しても set2/set3 は XDV でも有効である。

JFM はメトリック(組版)専用とし、グリフ描画はレンダラ側に置く。
TFM 型の和文ビットマップフォントは対象外である。

## 和文ノードと native_word の境界規則

XeTeX 層([xetex.md](xetex.md))の native_word と和文ノードの相互作用を次のように定める。

1. 収集の分離：collect_native は「現在フォントが native」の欧文経路である。
   和文文字(kcatcode が kanji/kana)は、現在の和文カレントフォント(欧文フォントとは独立したカレント。pTeX の cur_jfont 相当)で 2 連 char_node になる。
   1 つの native_word の中に和文文字は決して入らない。
2. 境界グルー：native_word と和文ノードの境界は、欧文と和文の境界として \xkanjiskip の挿入対象になる。
   判定は native_word の端の文字ではなくノード種別で行う(native_word 全体を「欧文塊」と見る)。
3. 行分割：native_word の内部は分割しない。
   和文文字間と境界のみが新しい分割点である。
4. 禁則：\prebreakpenalty 等は和文文字コードで引く。
   native_word に隣接する側にも適用される(例：開き括弧が native_word の直後)。

## 検証基準

- euptex で同一入力の DVI を作り、和文ボックスの寸法、グルー、ペナルティ列を \showbox レベルで比較する(XDV と pDVI の比較になるため、バイト比較ではなくノード表示で突き合わせる)。
- 和文未使用の文書では、XeTeX 層と XDV バイト一致(透過性)。

## 既知の制限

- \jcharwidowpenalty の挿入(パラメータは存在するが、adjust_hlist の pf=true 経路が未実装)
- disp_node(showbox の `\displace 0.0`)と box の `, yoko direction` 表示。
  euptex との showbox 完全一致には、この 2 つの表示差が残る(数値とノード列は一致)
- 縦組み(tate:dir_node、\tate/\yoko、tate JFM id=9 の組版)
- \unhbox 時の cur_kanji_skip 復元(space_ptr(head) 相当)
- adjust_hlist の hbox surround spacing(箱の中身の先頭と末尾の文字との境界)、accent kern 経路、math surround の厳密化

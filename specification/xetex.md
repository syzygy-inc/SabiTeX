# XeTeX 層の設計

一次資料は `reference/xetex/xetex.web`(TeX Live の生成済み web、34,428 行)と、C 側の XDV シリアライズとフォントロードを担う `reference/xetex/XeTeX_ext.c` / `XeTeX_ext.h` である。
本ポートでは C/C++ 層(ICU、HarfBuzz、フォントマネージャ)を rustybuzz(feature `shaping`)で置き換える。
実装の中心は `native.rs` と `xemath.rs`。

## XDV フォーマット(id_byte = 7)

DVI との差分は opcode 252-254 のみである。
他は classic DVI と同一で、TRIP と e-TRIP の DVI ライタをそのまま共有できる。

```
define_native_font = 252:
  k[4]  フォント番号 (f - font_base - 1)
  s[4]  サイズ (sp)
  flags[2]
  l[1] n[l]      フォントファイル名 (UTF-8、":features" は含まない)
  i[4]  face index
  if flags & COLORED  (0x0200): rgba[4]
  if flags & EXTEND   (0x1000): extend[4]   (Fixed)
  if flags & SLANT    (0x2000): slant[4]    (Fixed)
  if flags & EMBOLDEN (0x4000): embolden[4] (Fixed)
  VERTICAL = 0x0100
  (font_flags 側: COLORED=0x01, VERTICAL=0x02 — XDV flags とは別物)

set_glyphs = 253:
  w[4]   ノード幅 (sp)
  k[2]   グリフ数
  xy[8k] 各グリフの x[4] y[4] (sp、ノード原点基準。y は下向き正)
  g[2k]  glyph ID

set_text_and_glyphs = 254 (native_word_node_AT = ActualText 用):
  l[2] t[2l]  元テキスト (UTF-16)
  ... 以降 set_glyphs と同じ w/k/xy/g
```

vlist 中の glyph_node は `set_glyphs w=0, k=1, x=y=0` で出力し、cur_v は height/depth で進める。
hlist 中は幅 width(p) で cur_h を進める。

## ノード(whatsit subtype)

```
native_word_node    = 40   native_node_size = 6 + ⌈len/2⌉ (UTF-16 2 文字/word)
native_word_node_AT = 41
glyph_node          = 42   glyph_node_size = 5

native_word:
  [1-3] width/depth/height (box と同じ)
  [4]   qqqq: b0=native_size(語数) b1=native_font b2=native_length(UTF-16 長)
             b3=native_glyph_count
  [5]   native_glyph_info_ptr — C ポインタ。本ポートでは mem 外の
        サイドテーブル (Engine 内 HashMap<Pointer, Vec<GlyphInfo>>) で
        代替し、copy/free (copy_native_glyph_info / free_native_glyph_info)
        を明示フックする。GlyphInfo = { x: Scaled, y: Scaled, gid: u16 }
        (C 版は 10 bytes/glyph: FixedPoint x,y + u16 gid)
  [6..] UTF-16 テキスト

glyph_node: [4].b1=native_font、[4].b2=native_glyph (=glyph ID)
```

pic_node (43) と pdf_node (44)(\XeTeXpicfile / \XeTeXpdffile、ビットマップ画像の \includegraphics)は未実装である。

## native font のロード(load_native_font)

- `\font\x="名前:features" at 10pt` の形で指定する。
  font_name には canonical 名(features 込み)を格納する。
  DVI の fnt_def へは ":" 以降を切り落として出力する(dvi_font_def の名前出力は最初の ":" で打ち切る)。
- font_area は文字列でなくフラグ値である。
  otgr_font_flag = 0xFFFE(aat = 0xFFFF は macOS のみで、本ポートは otgr のみ)。
  is_native_font(f) は、font_area が 0xFFFE/0xFFFF であること。
- メトリクス：ascent → height_base[f]、-descent → depth_base[f](native はフォント単位の 1 値で、文字ごとの h/d は使わない)。
- fontdimen 1..8 = slant、space(スペース幅+letter_space)、space/2、space/3、x_height、quad = font_size、space/3、cap_height。
  bc=0、ec=65535。
- font_letter_space、font_mapping、font_flags を別配列で管理する。

## 実装方針(sabitex 固有)

- シェーピングは rustybuzz::shape で行う。
  グリフ位置は shaping 結果の advance/offset から sp へ `xn_over_d(units, size, upem)` で換算する。
- find_native_font 相当：TexFs 経由でフォントファイルのバイト列を取得する。
  kpathsea/fontconfig の名前解決はしない(manifest 主義)。
  "[path]" 形式のファイル名指定を一次サポートとする。
- features 構文(":" 区切り)は、`+liga` 等の on/off と `letterspace=`、`color=RRGGBBAA` の最小集合。
- XDV への切替：DviState に `id_byte` を持たせ、native font が 1 つでも定義されたら 7、さもなくば 2 とする。
  実際の XeTeX は -no-pdf 時常時 7 だが、TRIP と e-TRIP は classic DVI 比較なので、engine 既定は 2 を維持し、native font のロードで 7 に昇格させる(和文の set2/set3 出力は [japanese.md](japanese.md) を参照)。

## main loop

char が native font のとき、`collect_native`(xetex.web §24214-)で連続文字列を貯めて native_word_node 化し、set_native_metrics(= シェーピング)を行う。
スペースは font の space param による glue。

## XeTeX からの簡略化(記録)

意図的に落としている、または簡略化している機能を挙げる。
`native.rs` の該当箇所にもコメントで明示する。

- AAT(macOS 固有)は対象外。
  Graphite も対象外(rustybuzz の OT シェーピングのみ)。
- TECkit mapping(font_mapping)、\XeTeXlinebreaklocale、ダッシュ位置での行分割(dash-break)、単語間シェーピング(interword shaping)、隣接 native_word の merge は未実装。
- OpenType MATH(MathConstants / fontdimen9+)は未対応(数式は TFM のみ)。
- pic/pdf ノードとビットマップ画像出力は未実装(前節を参照)。

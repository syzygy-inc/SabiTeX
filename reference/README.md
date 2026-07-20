# 第三者由来の参照データ

他のソフトウェアから取り込んだ一次仕様、テストデータ、フォントを、ソフトウェア別にこのディレクトリへ置く。
各ファイルは元のライセンスに従う(本リポジトリ自体のライセンスは適用されない)。

| ディレクトリ | 由来 | 用途 |
|---|---|---|
| `tex/` | Knuth の TeX82(CTAN systems/knuth/dist/tex)。`tex.web` 本体と TRIP テスト一式(`trip/`。trip.tfm は `pltotf trip.pl` で生成) | 一次仕様と適合性テスト |
| `etex/` | e-TeX(`etex.ch`)と e-TRIP テスト一式(`etrip/`) | 一次仕様と適合性テスト |
| `xetex/` | XeTeX(TeX Live)。生成済み `xetex.web` と C 側ソース(`XeTeX_ext.c` ほか) | 一次仕様 |
| `ptex/` | pTeX(texjporg)。`ptex-base.ch` | 一次仕様 |
| `uptex/` | upTeX(texjporg)。`uptex-m.ch` と和文フォントメトリック `umin10.tfm` | 一次仕様とテストフィクスチャ |
| `computer-modern/` | Knuth の Computer Modern フォント。`cmr10.tfm` | テストフィクスチャ |
| `latin-modern/` | GUST の Latin Modern フォント(`GUST-FONT-LICENSE.TXT` 同梱) | テストフィクスチャとサンプル |
| `pgf/` | PGF/TikZ 公式マニュアル(組版済み PDF) | レンダリング精度の照合基準 |
| `dvipdfmx/` | dvipdfmx(TeX Live)の special 処理ソース | \special 意味論の参照 |

tex.web は Knuth の著作物であり、改変した派生物を「TeX」と名乗ることは許されない(変更は .ch ファイル方式で行うのが慣例)。
本プロジェクトはこれらを改変せず、参照専用として保持する。

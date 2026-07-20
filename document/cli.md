# CLI(`sabitex`)の使い方

```
sabitex [--fmt <file.fmt>] [--interaction=<mode>] [<first line>]
sabitex --version | -V
```

伝統的な TeX のコマンドラインを踏襲する。
`<first line>` は TeX の `**` プロンプトに打つ 1 行と同じ扱いで、次のように解釈される。

- `sabitex story`：先頭が `\` `&` `*` 以外なら `\input story` に展開する
- `sabitex "&plain story"`：フォーマット `plain.fmt` をロードして `story.tex` を処理する(`--fmt plain.fmt` との併用も可)
- `sabitex "*\relax ..."` や `sabitex "\input story"`：そのまま解釈する
- 引数なし：`**` プロンプトを表示し、標準入力から読む

## オプション

| オプション | 意味 |
|---|---|
| `--fmt <path>` | フォーマットファイルを直接パス指定でロードする(kpsewhich は経由しない) |
| `--interaction=<mode>` | `batchmode` / `nonstopmode` / `scrollmode` / `errorstopmode` |
| `--version`, `-V` | バナーを表示して終了する |

## ファイル解決

入力ファイル(.tex、.tfm、.sty、フォント等)は次の順で探す。

1. カレントディレクトリ(相対パス指定もここで解決する)
2. インストール済み TeX Live。
   `kpsewhich -var-value TEXMFDBS` の ls-R データベースを一度だけ索引化して引く。
   索引外(生成ファイル、エイリアス、フォントマップ)は個別に `kpsewhich` へフォールバックする

TeX Live がなくても、必要なファイルをカレントディレクトリ(または相対パスで届く場所)に置けば動く。
環境変数 `SABITEX_TRACE_FILES` をセットすると、各ファイルがどこで解決されたかを stderr に表示する。

## フォーマットの生成(INITEX)

`--fmt` なしで起動すると、INITEX 相当(プリミティブのみ)で始まる。
`\dump` でフォーマットを書き出せる。

```
sabitex "*\input plain \dump"     # plain.tex から plain.fmt を生成
sabitex "&plain story"            # 生成した fmt で story.tex を処理
```

フォーマットは独自形式であり、他の TeX 処理系の .fmt とは互換性がない(`specification/architecture.md` を参照)。

## 出力

- 組版結果は DVI である(native font を使うと XDV に昇格する。`specification/xetex.md`)。
  `xdvipdfmx` で PDF にできる。
- 転写は `<jobname>.log` に逐次書き出される。
- 日時(`\year` 等)には実時刻が注入される。

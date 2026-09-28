# 契約 case 台帳

品質保証指針 `qa-v0`（`SabiSeries/qa/`）の [記録テンプレート](../../SabiSeries/qa/record-template.md) に対応する、このリポジトリの予定 case。
[qa.md](qa.md) の「TRIP/e-TRIP の内容証拠」「skip の禁止」を実装する。各 case は `crates/sabitex-qa` の `Case` で台帳
（`target/qa-ledger/*.tsv`、または `SABI_QA_LEDGER`）に PASS / FAIL / BLOCKED / NOT-RUN と比較件数を残し、`scripts/qa-ledger.sh` が本表と突き合わせる。

`required` は oracle プロファイルで必須（参照環境が無ければ BLOCKED で失敗。`SABI_STRICT_TESTS`）。参照ツールが起動したのに出力を出さなければ
FAIL（環境不足ではなく検査の失敗）。テストを `return` や `eprintln!("SKIPPED")` で抜けて緑にしない。

## 台帳に載せる case

| case | 契約 | プロファイル | 必須 | 参照資源 | 内容 |
|---|---|---|---|---|---|
| TEX-TRIP-INITEX | C-TEX | trip | required | （同梱: reference/tex/trip） | tripman §2: INITEX パスの転写を tripin.log と比較、trip.fmt を dump |
| TEX-TRIP-VIRTEX | C-TEX, C-DVI | trip | required | （同梱） | tripman §3: `&trip trip` の転写を trip.log、端末を trip.fot と比較。trip.dvi は 2920 bytes |
| TEX-TRIP-DVITYPE | C-DVI | trip-dvitype | required | dvitype | tripman §4: `dvitype -output-level=2 -dpi=72.27` の出力を trip.typ とバナー以外で全行比較 |
| TEX-ETRIP-INITEX | C-TEX | etrip | required | （同梱: reference/etex/etrip） | e-TRIP の INITEX パス（`*etrip`）を etripin.log と比較 |
| TEX-ETRIP-VIRTEX | C-TEX, C-DVI | etrip | required | （同梱） | `&etrip etrip` の転写・端末・etrip.out を比較。etrip.dvi は 220 bytes |
| TEX-ETRIP-DVITYPE | C-DVI | trip-dvitype | required | dvitype | dvitype の出力を etrip.typ とバナーと DVI コメント（日付）以外で全行比較 |
| TEX-GOLDEN-EMPTY | C-DVI, C-TEX | dvi-golden | required | tex | 空の hbox のページ。Knuth の tex（INITEX）の DVI とバイト一致 |
| TEX-GOLDEN-PARAGRAPH | C-DVI, C-TEX | dvi-golden | required | tex、cmr10.tfm | 段落の行分割とページ組立 |
| TEX-GOLDEN-STORY | C-DVI, C-TEX | dvi-golden | required | tex、plain.tex、story.tex | plain + story.tex |
| TEX-GOLDEN-MATH | C-DVI, C-TEX | dvi-golden | required | tex、plain.tex | 分数・根号・区切り・limits・displaylines |
| TEX-GOLDEN-ALIGN | C-DVI, C-TEX | dvi-golden | required | tex、plain.tex | halign（span、noalign、omit）、eqalign、settabs |
| TEX-GOLDEN-FMT | C-DVI, C-JOB | dvi-golden | required | tex、plain.tex、story.tex | plain を dump し別エンジンで load して story.tex を組む（fmt の producer/consumer） |
| TEX-GOLDEN-READ | C-DVI, C-TEX | dvi-golden | required | tex、cmr10.tfm | openin / read / ifeof |
| TEX-GOLDEN-DISC | C-DVI, C-TEX | dvi-golden | required | tex、plain.tex | discretionary と明示ハイフン |
| TEX-GOLDEN-LIGKERN | C-DVI, C-TEX | dvi-golden | required | tex、cmr10.tfm | 合字とカーン |
| TEX-GOLDEN-GLUE | C-DVI, C-TEX | dvi-golden | required | tex、cmr10.tfm | グルーの設定と罫線 |
| TEX-GOLDEN-VBOX | C-DVI, C-TEX | dvi-golden | required | tex、cmr10.tfm | vbox の基線とカーン |
| TEX-GOLDEN-BOXES | C-DVI, C-TEX | dvi-golden | required | tex、cmr10.tfm | box register、copy、raise、leaders |
| TEX-GOLDEN-ATSIZE | C-DVI, C-TEX | dvi-golden | required | tex、cmr10.tfm | at / scaled 指定のフォントと空白 |

TEX-TRIP-VIRTEX と TEX-TRIP-DVITYPE（e-TRIP も同様）は同じテスト関数で実行する。後者は前者が書いた `target/trip/ours-trip.dvi` を
`dvitype` に掛ける。転写の比較規則（正当差分、マスク行）は [trip.md](trip.md) / [etrip.md](etrip.md) が正本で、台帳はその実施を記録する。

## 常に実行する検査（外部資源に依らない。台帳には載せない）

`cargo test --workspace` の単体・統合テスト: scaled 算術（`arith.rs`、`print_scaled.rs`）、展開（`expansion.rs`）、e-TeX 拡張（`etex.rs`）、
memory arena（`mem_arena.rs`）、fmt codec の単体テスト（`fmt.rs`）と fmt の往復・切断・不正長・poisoned（`fmt_roundtrip.rs`）、
wasm ABI の操作列（`sabitex-wasm`）、CLI の `&format` 解決（`sabitex-cli/tests/format_spec.rs`）。

## 参照資源のロック

oracle プロファイルの基準は TeX Live 2025 の配布物（`tex`、`dvitype`、`kpsewhich`）。CI は Ubuntu の `texlive-binaries` / `texlive-base` /
`texlive-plain-generic` を導入する（`.github/workflows/ci.yml`）。参照ファイルの sha256 は [resources.lock](resources.lock) に固定し、
`scripts/qa-resources.sh check` が kpsewhich で解決したファイルの digest を照合する（必須資源の不在と digest の相違は失敗。ツールの版は表示のみ）。
基準を更新するときは手元の TeX Live で `scripts/qa-resources.sh record` を実行し、差分を変更管理に載せる。TRIP / e-TRIP の参照ファイルは
リポジトリに同梱している（`reference/`）。

## 未保証（保証済みに数えない）

- Unicode・XeTeX・和文拡張の専用 case（[qa.md](qa.md)）はまだ無い。`etex.rs` の拡張テストは手計算の期待値による単体テストで、
  参照処理系との比較ではない。
- TFM/JFM の SabiFace との照合（同じ parser を両側から呼ばない独立した期待値）は未着手。
- 形式検証（Lean）は未導入。

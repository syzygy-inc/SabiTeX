# SabiTeX アーキテクチャ

tex.web(Knuth の TeX82)を一次仕様とする Rust 再実装である。
npTeX 方式の日本語対応 XeTeX(TeX82 + e-TeX + XeTeX 拡張 + pTeX/upTeX 互換の和文組版)を、TRIP / e-TRIP テスト準拠かつ WASM ビルド可能な小さな処理系として実装する。
出力は DVI / XDV である。
tex.web の Part と Rust モジュールの対応は [texweb-map.md](texweb-map.md) を、一次資料の所在は `reference/` を参照。

## クレート構成

| クレート | 役割 |
|---|---|
| `sabitex-core` | エンジン本体。依存ゼロと wasm32 クリーンを維持し、I/O は trait 経由のみ。TFM/JFM リーダ、DVI/XDV ライタ、OpenType シェーピング(feature `shaping` = rustybuzz)も core 内のモジュール |
| `sabitex-cli` | ネイティブ CLI(バイナリ名 `sabitex`)。`shaping` を有効化する |
| `sabitex-wasm` | wasm バインディング。wasm-bindgen を使わない手書き C ABI とメモリ VFS |

**feature でセマンティクスを切り替えない。**
e-TeX、XeTeX、和文拡張は実物のエンジン同様、単一バイナリ内のランタイム透過(拡張未使用なら挙動同一)で実現する。
feature は重い依存のゲート(`shaping` = rustybuzz)に限る。

## コア設計判断

### 1. `mem[]` の忠実移植(enum ノード化はしない)

TRIP の log はアロケータ統計やノード走査順に依存し、tex.web は「mem インデックス = ポインタ同一性」を多用する。
このため `mem` は `Vec<MemoryWord>` + `i32` インデックスとして直訳する(`mem.rs`)。
`MemoryWord` は `u64` 1 本とビット操作アクセサで表す(`memword.rs`)。
halfword = `i32`、quarterword = `u16`(XeTeX レイアウト)。
`glue_set` は `f64`(web2c 既定)だが、格納は f32 経由とする(web2c の `glue_ratio` は単精度であり、TRIP の glue set 表示と DVI 丸めの一致に必要)。
TeX Live の e-TeX 系バイナリに合わせる場合は `Mem::glue_ratio_wide = true` とする(e-TRIP ハーネスが使用)。
浮動小数を触るのは glue set 関係のモジュールに隔離する。

### 2. 文字は最初から UTF-32

`UnicodeChar = i32` とする。
8bit で書いてから widen するリファクタは全域に波及するため、最初から XeTeX 型で書く。
文字列プールは UTF-16(XeTeX 同様)。
TRIP は XeTeX と同じくこの上で通し、既知差分はマスクルールとして管理する([trip.md](trip.md))。

### 3. 型エイリアス(newtype にしない)

`Scaled = i32` と `Pointer = i32` は意図的にプレーンなエイリアスである。
tex.web は `sc == int`(§113)で整数とスケール値を同一視し、`q := p + node_size(p)` 等のポインタ演算が遍在するため、newtype はポートの可読性(tex.web との 1:1 対応)を損なう。
安全性は代わりに次の二つで担保する。

- スケール値の乗除は必ず `arith.rs` のプリミティブを経由する(生の `*` や `div` は使わない。加減算は tex.web §104 と同じく無検査)。
- `mem` アクセスは必ず `Mem` のアクセサメソッドを経由する(`link`/`info`/`node_size` などが tex.web のマクロと 1:1 対応)。

### 4. グローバル状態は単一 `Engine`

tex.web の数百のグローバル変数は `Engine` 構造体(`engine.rs`)にサブシステム単位で集約し、全手続きを `&mut self` メソッドにする。
`Sizes` は実行時パラメータである(TRIP は小メモリ設定を要求するため可変とする。実用値は `Sizes::production()`)。

### 5. Pascal goto の変換規約

| tex.web | Rust |
|---|---|
| `restart:` / `goto restart` | `'restart: loop { … continue 'restart; }` |
| `reswitch:` | `'reswitch: loop { match … { continue 'reswitch } }` |
| `goto done / found / not_found` | labeled block `'done: { … break 'done; }`(またはヘルパー関数化) |
| `goto exit` | 早期 `return` |
| `jump_out` / `overflow` / `fatal_error` | `Result<T, TexInterrupt>` を main_control まで伝播 |

### 6. I/O 抽象(`io.rs`)

エンジンは `TexFs`(ファイル全量読みと追記書き)と `Terminal` の 2 trait しか知らない。
ファイル名解決(kpathsea 相当)はエンジン外の責務である(CLI は TeX Live の ls-R 索引と kpsewhich フォールバックで解決する)。
WASM はメモリ VFS と事前バンドルで同期 I/O のまま動かす(不足ファイルは missing-list をホストが取得して再実行する)。
Asyncify は使わない。

### 7. フォーマットファイル

tex.web 非互換の独自固定幅 LE 形式である(`fmt.rs`)。
ネイティブ(64bit)で焼いた fmt を wasm(32bit)で読むため、`usize` やポインタを直接ダンプしない。
`MemoryWord::bits()` の `u64` を LE で書く。
fmt のバイト互換は保証対象外とする(TRIP でも trip.fmt 自体の比較は対象外とする設計判断)。

### 8. 決定性

乱数、時刻、HashMap の乱数シードに依存しない(時刻は `Engine::set_date_and_time` 経由で、既定は固定日付。連想配列は BTreeMap か固定シード)。
同一入力から同一出力が得られることをテストの前提とする。
CLI は実時刻を、wasm は `sabitex_set_time` をオプトインで注入する。

## tex.web からの意図的な逸脱(記録)

| 逸脱 | 理由 | 影響 |
|---|---|---|
| TEX.POOL を読まない(文字列は起動時に intern) | WEB の tangle 工程が存在しない | 文字列の番号が tex.web と異なる。fmt は独自形式なので非互換問題なし |
| `min_quarterword`/`min_halfword` = 0 固定、halfword=i32/quarterword=u16 | XeTeX レイアウト | tex.web の `qi`/`qo`/`hi`/`ho` は恒等写像になる |
| `mem_min = mem_bot` は `Sizes` で可変(既定 0) | 推奨設定(§12)。TRIP は 1 を要求する | なし |
| 文字列プールが UTF-16 | XeTeX 互換 | BMP 外は XeTeX の仮想文字列方式 |
| `print_roman_int` 等の WEB プール文字列リテラルを Rust バイト列で表現 | 意味同一 | なし |

XeTeX 層の簡略化は [xetex.md](xetex.md) に、和文組版の設計は [japanese.md](japanese.md) に記録する。

## テスト方針

- **単体 golden**：arith、print_scaled、mem は tex.web の手計算値と一致させる(`crates/sabitex-core/tests/`)。
- **DVI golden**：実 TeX との DVI バイト比較(`dvi_golden.rs`)。
- **適合性**：TRIP と e-TRIP。
  入力一式は `reference/tex/trip/` と `reference/etex/etrip/` に vendoring 済みで、ハーネスは `tests/trip.rs` と `tests/etrip.rs`、比較規則は [trip.md](trip.md) と [etrip.md](etrip.md)。

CI(`.github/workflows/ci.yml`)は fmt、clippy、全テスト、wasm32 ビルドを検証する。

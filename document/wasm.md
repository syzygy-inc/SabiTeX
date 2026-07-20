# wasm バインディング(`sabitex-wasm`)の使い方

wasm-bindgen を使わない手書きの C ABI である。
ホスト(JS 等)がバッファを確保し、仮想ファイルシステム(VFS)にファイルを登録してコンパイルを実行し、出力を読み戻す。
import なしでインスタンス化できる。

```
cargo build --release -p sabitex-wasm --target wasm32-unknown-unknown
# → target/wasm32-unknown-unknown/release/sabitex_wasm.wasm
```

## エクスポート一覧

| 関数 | 役割 |
|---|---|
| `sabitex_alloc(len) -> ptr` / `sabitex_free(ptr, len)` | ホストが書き込むバッファの確保と解放(free は同じ長さで呼ぶ) |
| `sabitex_vfs_add(name_ptr, name_len, data_ptr, data_len)` | 仮想ファイルの登録(同名は置換) |
| `sabitex_prepare(fmt_ptr, fmt_len) -> rc` | エンジン構築とフォーマットロード。`fmt` は VFS 上の名前(空文字列 = INITEX)。0=成功、2=フォーマットロード失敗 |
| `sabitex_run(first_ptr, first_len) -> rc` | prepare 済みエンジンで 1 ジョブ実行する(`first` は `**` 行)。0=成功、1=エラー終了、2=prepare されていない |
| `sabitex_compile(first_ptr, first_len, fmt_ptr, fmt_len) -> rc` | prepare と run の一括版 |
| `sabitex_output_len(name_ptr, name_len) -> len` / `sabitex_output_ptr() -> ptr` | 名前付き出力の取得(ptr は次の呼び出しまで有効) |
| `sabitex_set_time(year, month, day, minutes)` | `\year` 等に使う時刻の注入(呼ばなければ固定日付で決定的) |
| `sabitex_init_panic_hook()` | パニック文言を `<panic>` 出力に記録するフックの設置 |

prepare と run の分割は、フォーマットのロード(undump)を先行して行っておくための仕組みである。
1 回の `sabitex_prepare` につき 1 回だけ `sabitex_run` できる(ジョブごとに作り直す)。

## 出力の名前

`sabitex_output_len` / `sabitex_output_ptr` には、実ファイル名(`doc.dvi`、`doc.log` など)のほか、次の仮想名を渡せる。

| 名前 | 内容 |
|---|---|
| `<terminal>` | 端末出力(転写とは別) |
| `<missing>` | VFS に無くて読めなかったファイル名の改行区切りリスト |
| `<outputs>` | 直前のジョブが生成した全出力名の改行区切りリスト |
| `<stats>` | `{"errorCount":N,"ok":bool}` の JSON |
| `<panic>` | パニック時のメッセージ(要 `sabitex_init_panic_hook`) |

## missing-file プロトコル

VFS に全ファイルを事前バンドルする必要はない。
1 パス走らせ、`<missing>` に出た名前をホストが取得して `sabitex_vfs_add` し、再実行する(SwiftLaTeX 方式)。
エンジンは足りないファイルのプロンプトに空行を返して継続するため、1 パスですべての不足ファイルが列挙される。

## 最小の JS 例

```js
const { instance } = await WebAssembly.instantiate(wasmBytes, {});
const ex = instance.exports;
const enc = new TextEncoder();

const put = (bytes) => {
  const ptr = ex.sabitex_alloc(bytes.length);
  new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
  return [ptr, bytes.length];
};
const withBuf = (bytes, f) => {
  const [p, l] = put(bytes);
  try { return f(p, l); } finally { ex.sabitex_free(p, l); }
};
const vfsAdd = (name, data) =>
  withBuf(enc.encode(name), (np, nl) =>
    withBuf(data, (dp, dl) => ex.sabitex_vfs_add(np, nl, dp, dl)));
const output = (name) =>
  withBuf(enc.encode(name), (np, nl) => {
    const len = ex.sabitex_output_len(np, nl);
    return new Uint8Array(ex.memory.buffer, ex.sabitex_output_ptr(), len).slice();
  });

ex.sabitex_init_panic_hook();
vfsAdd("doc.tex", enc.encode(source));          // 入力を登録
// フォーマットを使う場合: vfsAdd("plain.fmt", fmtBytes)
const rc = withBuf(enc.encode("\\input doc"), (fp, fl) =>
  withBuf(enc.encode(""), (mp, ml) =>          // "" = INITEX
    ex.sabitex_compile(fp, fl, mp, ml)));

const dvi = output("doc.dvi");
const log = new TextDecoder().decode(output("doc.log"));
const missing = new TextDecoder().decode(output("<missing>"));
```

フォーマットファイルは、ネイティブ CLI で生成したもの([cli.md](cli.md))をそのまま VFS に載せられる。
64bit で焼いた fmt を 32bit の wasm で読める独自 LE 形式である(`specification/architecture.md` を参照)。

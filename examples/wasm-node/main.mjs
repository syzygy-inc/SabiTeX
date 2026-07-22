// wasm ABI を node から直接叩く最小例(ブラウザ不要)。
//
// missing-file プロトコルのデモを兼ねる:
//   1 パス目: 入力 .tex だけを VFS に入れて実行 → <missing> に不足
//   ファイル (cmr10.tfm) が列挙される
//   2 パス目: 不足分を追加して再実行 → doc.dvi が得られる
//
// 事前に wasm をビルドしておく:
//   cargo build --release -p sabitex-wasm --target wasm32-unknown-unknown
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const dir = dirname(fileURLToPath(import.meta.url));
const wasmPath = join(
  dir,
  "../../target/wasm32-unknown-unknown/release/sabitex_wasm.wasm",
);
const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {});
const ex = instance.exports;
const enc = new TextEncoder();
const dec = new TextDecoder();

const put = (bytes) => {
  const ptr = ex.sabitex_alloc(bytes.length);
  new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
  return [ptr, bytes.length];
};
const withBuf = (bytes, f) => {
  const [p, l] = put(bytes);
  try {
    return f(p, l);
  } finally {
    ex.sabitex_free(p, l);
  }
};
const vfsAdd = (name, data) =>
  withBuf(enc.encode(name), (np, nl) =>
    withBuf(data, (dp, dl) => ex.sabitex_vfs_add(np, nl, dp, dl)),
  );
const output = (name) =>
  withBuf(enc.encode(name), (np, nl) => {
    const len = ex.sabitex_output_len(np, nl);
    return new Uint8Array(ex.memory.buffer, ex.sabitex_output_ptr(), len).slice();
  });
const compile = (first) =>
  withBuf(enc.encode(first), (fp, fl) =>
    withBuf(enc.encode(""), (mp, ml) => ex.sabitex_compile(fp, fl, mp, ml)),
  );

const SRC = [
  "\\catcode`\\{=1 \\catcode`\\}=2",
  "\\font\\rm=cmr10 \\rm",
  "\\shipout\\hbox{Hello from wasm!}",
  "\\end",
  "",
].join("\n");

ex.sabitex_init_panic_hook();
vfsAdd("doc.tex", enc.encode(SRC));

// 1 パス目: cmr10.tfm が無いまま実行し、不足リストを得る。
let rc = compile("\\input doc");
const missing = dec.decode(output("<missing>")).split("\n").filter(Boolean);
console.log(`pass 1: rc=${rc}, missing = [${missing.join(", ")}]`);

// 2 パス目: 不足分をリポジトリ内の TFM で埋めて再実行する。
vfsAdd(
  "cmr10.tfm",
  readFileSync(join(dir, "../../reference/computer-modern/cmr10.tfm")),
);
rc = compile("\\input doc");
const stats = dec.decode(output("<stats>"));
const dvi = output("doc.dvi");
console.log(`pass 2: rc=${rc}, stats=${stats}, doc.dvi ${dvi.length} bytes`);
if (rc !== 0 || dvi.length === 0) {
  console.error("compile failed");
  process.exit(1);
}
writeFileSync(join(dir, "doc.dvi"), dvi);
console.log("wrote doc.dvi (id_byte=" + dvi[1] + ")");

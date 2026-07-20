**English** | [日本語](README.ja.md)

# SabiTeX

A Rust reimplementation of the TeX typesetting engine, with tex.web (Knuth's TeX82) as its primary specification.
It implements TeX82 plus e-TeX, the XeTeX extensions, and pTeX/upTeX-compatible Japanese typesetting (the npTeX approach) in a single engine, and outputs DVI / XDV.

The name comes from the Japanese word *sabi*: 錆び ("rust") and 寂び (the quiet, weathered beauty behind *wabi-sabi*).
It carries the intent to keep maintaining this engine until it ages gracefully and gathers moss.
In Japanese it is read as さびてふ or さびてく.

- **TRIP / e-TRIP conformance**: reproduces Knuth's TRIP and e-TeX's e-TRIP in both the INITEX and VIRTEX passes.
  The comparison rules are recorded in `specification/trip.md` and `specification/etrip.md`.
- **Dependency-free core**: `sabitex-core` has no external dependencies and runs on wasm32 as is.
  Only OpenType shaping relies on the optional rustybuzz.
- **Japanese typesetting**: JFM-based typesetting, \kanjiskip / \xkanjiskip, and kinsoku (line-breaking prohibition) rules are implemented at the engine level, following pTeX/upTeX (`specification/japanese.md`).

## Quick start

```
cargo build --release -p sabitex-cli
cd examples/hello
../../target/release/sabitex hello     # → hello.dvi (XDV)
```

Self-contained samples that run without a TeX Live installation live in [examples/](examples/README.md).
See [document/cli.md](document/cli.md) for the CLI and [document/wasm.md](document/wasm.md) for use from the browser / wasm.

## Repository layout

| Path | Contents |
|---|---|
| `crates/sabitex-core` | The engine itself (zero dependencies; I/O only through traits) |
| `crates/sabitex-cli` | Native CLI (binary name `sabitex`) |
| `crates/sabitex-wasm` | wasm bindings (hand-rolled C ABI) |
| `examples/` | Self-contained samples (Latin OpenType and Japanese JFM) |
| `document/` | Usage guides (building, CLI, wasm) |
| `specification/` | Records of design decisions, behavioral specifications, and test comparison rules |
| `reference/` | Third-party primary sources and test data (grouped per software; each file keeps its own license) |

Documentation under `document/` and `specification/` is currently written in Japanese.

## Building and testing

```
cargo test --workspace          # all tests, including TRIP / e-TRIP
```

See [document/building.md](document/building.md) for details.

## License

See `LICENSE` for the license of this project's code.
Third-party files under `reference/` are not covered by it and remain under their original licenses (`reference/README.md`).

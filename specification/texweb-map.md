# tex.web → Rust モジュール対応表

tex.web(`reference/tex/tex.web`、全 55 Part、25,010 行)の各 Part と SabiTeX のモジュールの対応表である。
**レビューは必ずこの表から該当 § を引き、tex.web の記述と突き合わせて行う。**

e-TeX、XeTeX、pTeX と upTeX の拡張はそれぞれ `reference/etex/etex.ch`、`reference/xetex/xetex.web`、`reference/ptex/ptex-base.ch` と `reference/uptex/uptex-m.ch` を一次資料とする。
拡張の対応モジュールは `expr.rs` と `sa.rs`(e-TeX)、`native.rs` と `xemath.rs`(XeTeX)、`kanji.rs`(和文)が中心となる(設計は [xetex.md](xetex.md) と [japanese.md](japanese.md))。

| Part | 内容 | 開始行 | Rust モジュール |
|---|---|---|---|
| 1 | Introduction | 95 | `lib.rs`(banner) |
| 2 | The character set | 523 | (xord/xchr は恒等。UTF-32 ネイティブ) |
| 3 | Input and output | 751 | `io.rs`(TexFs/Terminal trait に置換) |
| 4 | String handling | 1099 | `strings.rs` |
| 5 | On-line and off-line printing | 1383 | `print.rs` |
| 6 | Reporting errors | 1721 | `error.rs` |
| 7 | Arithmetic with scaled dimensions | 2144 | `arith.rs`, `print.rs`(print_scaled §103) |
| 8 | Packed data | 2375 | `memword.rs`, `types.rs` |
| 9 | Dynamic memory allocation | 2516 | `mem.rs` |
| 10 | Data structures for boxes | 2828 | `nodes.rs` |
| 11 | Memory layout | 3306 | `mem.rs`(§162-164) |
| 12 | Displaying boxes | 3517 | `boxops.rs`(show_box/short_display) |
| 13 | Destroying boxes | 3886 | `boxops.rs`(flush_node_list) |
| 14 | Copying boxes | 3960 | `boxops.rs`(copy_node_list) |
| 15 | The command codes | 4048 | `cmds.rs` |
| 16 | The semantic nest | 4220 | `nest.rs` |
| 17 | The table of equivalents (eqtb) | 4444 | `eqtb.rs`(USV 幅レイアウト) |
| 18 | The hash table | 5479 | `eqtb.rs`, `cmdchr.rs`(id_lookup/primitive) |
| 19 | Saving and restoring equivalents | 5813 | `eqtb.rs` |
| 20 | Token lists | 6151 | `tokens.rs` |
| 21-23 | Input stacks and states | 6334 | `input.rs` |
| 24 | Getting the next token | 7104 | `getnext.rs` |
| 25 | Expanding the next token | 7636 | `expand.rs` |
| 26 | Basic scanning subroutines | 8181 | `scan.rs` |
| 27 | Building token lists | 9142 | `toks.rs`(read_toks 含む) |
| 28 | Conditional processing | 9523 | `cond.rs` |
| 29 | File names | 9910 | `input.rs`(scan_file_name) |
| 30 | Font metric data | 10415 | `fonts.rs`(TFM/JFM リーダ。OpenType は `native.rs`) |
| 31 | DVI format | 11324 | `dvi.rs`(オペコード) |
| 32 | Shipping pages out | 11818 | `dvi.rs`(movement 最適化、hlist_out/vlist_out/ship_out) |
| 33 | Packaging (hpack/vpack) | 12857 | `pack.rs` |
| 34-36 | Math mode | 13329 | `math.rs`(noad 構造、var_delimiter、mlist_to_hlist、make_* 一式) |
| 37 | Alignment | 15108 | `align.rs`(preamble/init_row..fin_align/do_endv、span ノード) |
| 38-39 | Breaking paragraphs into lines | 15997 | `linebreak.rs`(try_break/post_line_break、active/delta ノード) |
| 40-43 | Hyphenation + trie | 17457 | `hyph.rs`(reconstitute/hyphenate/例外辞書/Liang trie + INITEX trie 構築) |
| 44-45 | Page breaking / page builder | 18872 | `page.rs`(vert_break/vsplit/build_page/fire_up/\output 再開) |
| 46 | The chief executive (main_control) | 19977 | `control.rs`(文字/リガチャ/カーン内側ループ含む、全モード分岐) |
| 47 | Building boxes and lists | 20502 | `control.rs` + `par.rs`(new_graf/end_graf/\unhbox/\insert/\mark/\accent/\//\discretionary) |
| 48 | Building math lists | 21679 | `mathlist.rs`(init_math/scan_math/sub_sup/fractions/\left\right/after_math/display) |
| 49 | Mode-independent processing | 22644 | `prefix.rs`(+\parshape/hyph_data/page_so_far/do_assignments) |
| 50 | Dumping and undumping | 23736 | `fmt.rs` + 各モジュールの dump/undump(独自固定幅 LE 形式) |
| 51 | The main program | 24222 | `sabitex-cli`(--fmt 読込)+ `control.rs`(final_cleanup §1335、open_log_file §534-§536) |
| 52 | Debugging | 24459 | (check_mem 相当はテストで代替) |
| 53 | Extensions | 24529 | `ext.rs`(\openout/\write/\closeout/\special/\immediate/\setlanguage、whatsit 全機構)+ `par.rs`(\language whatsit) |
| 54 | System-dependent changes | 24985 | `io.rs` ほか |

tex.web からの意図的な逸脱と簡略化は、[architecture.md](architecture.md) の「意図的な逸脱」表および [xetex.md](xetex.md) の簡略化一覧に記録する。

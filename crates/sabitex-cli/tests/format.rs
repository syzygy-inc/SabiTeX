//! The `&name` prefix on the first line loads `name.fmt` (§524, §1337).

use std::path::PathBuf;
use std::process::Command;

fn sabitex(dir: &PathBuf, line: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_sabitex"))
        .current_dir(dir)
        .arg("--interaction=nonstopmode")
        .arg(line)
        .output()
        .expect("run sabitex");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn ampersand_loads_the_named_format() {
    let dir = std::env::temp_dir().join(format!("sabitex-fmt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("mini.tex"),
        "\\catcode`\\{=1 \\catcode`\\}=2 \\def\\hello{HELLO}\\dump\n",
    )
    .unwrap();
    std::fs::write(dir.join("doc.tex"), "\\message{[\\hello]}\\end\n").unwrap();

    sabitex(&dir, "\\input mini");
    assert!(dir.join("mini.fmt").exists());

    // Both spellings load the format before doc.tex is read.
    assert!(sabitex(&dir, "&mini doc").contains("[HELLO]"));
    assert!(sabitex(&dir, "&mini.fmt doc").contains("[HELLO]"));

    let _ = std::fs::remove_dir_all(&dir);
}

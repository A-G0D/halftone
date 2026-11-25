//! Embeds every file in modules/ as a shipped module, so a new script needs no Rust edit.
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=modules");
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("modules");
    let mut names: Vec<String> = fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    let mut src = String::from("pub const BUILTIN: &[(&str, &str)] = &[\n");
    for n in names {
        src += &format!("    ({n:?}, include_str!({:?})),\n", root.join(&n).to_string_lossy());
    }
    src += "];\n";
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("builtin.rs"), src).unwrap();
}

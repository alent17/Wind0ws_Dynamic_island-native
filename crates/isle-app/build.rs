use std::{env, fs, path::PathBuf};

fn main() {
    // Ship the existing licensed font assets beside the executable. No font
    // installation, registry writes or dependency on the repository at runtime.
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest.join("../../../public/fonts");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let destination = out.ancestors().nth(3).unwrap().join("fonts");
    fs::create_dir_all(&destination).unwrap();
    for weight in ["Regular", "Medium", "Bold"] {
        let file = format!("MiSans-{weight}.ttf");
        let source = source.join(&file);
        println!("cargo:rerun-if-changed={}", source.display());
        fs::copy(source, destination.join(file)).unwrap();
    }
}

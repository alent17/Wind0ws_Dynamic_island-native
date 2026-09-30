use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    // Ship the existing licensed font assets beside the executable. No font
    // installation, registry writes or dependency on the repository at runtime.
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest.join("../../../public/fonts");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    embed_windows_identity(&manifest, &out);
    let destination = out.ancestors().nth(3).unwrap().join("fonts");
    fs::create_dir_all(&destination).unwrap();
    for weight in ["Regular", "Medium", "Bold"] {
        let file = format!("MiSans-{weight}.ttf");
        let source = source.join(&file);
        println!("cargo:rerun-if-changed={}", source.display());
        let target = destination.join(file);
        // DirectWrite maps these files while a test window is running. Avoid
        // rewriting unchanged font bytes during incremental builds.
        if fs::read(&source).ok() != fs::read(&target).ok() {
            fs::copy(source, target).unwrap();
        }
    }
    let icon_license = manifest.join("../../assets/icons/LICENSE");
    println!("cargo:rerun-if-changed={}", icon_license.display());
    fs::copy(
        icon_license,
        out.ancestors().nth(3).unwrap().join("LUCIDE-LICENSE.txt"),
    )
    .unwrap();
}

fn embed_windows_identity(manifest: &std::path::Path, out: &std::path::Path) {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numeric = format!("{},0", version.replace('.', ","));
    let icon = manifest.join("../../../src-tauri/icons/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let resource = out.join("identity.rc");
    let compiled = out.join("identity.res");
    fs::write(
        &resource,
        format!(
            r#"
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 1
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904b0"
  BEGIN
   VALUE "FileDescription", "Isle Native\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "ProductName", "Isle\0"
   VALUE "ProductVersion", "{version}\0"
   VALUE "OriginalFilename", "isle.exe\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x409, 1200
 END
END
"#,
            icon = icon.to_string_lossy().replace('\\', "/")
        ),
    )
    .unwrap();
    let sdk = PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK required"))
        .join("Windows Kits/10/bin");
    let mut compilers: Vec<_> = fs::read_dir(sdk)
        .expect("Windows SDK required")
        .flatten()
        .map(|entry| entry.path().join("x64/rc.exe"))
        .filter(|path| path.is_file())
        .collect();
    compilers.sort();
    let compiler = compilers
        .last()
        .expect("Windows SDK x64 resource compiler required");
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&compiled)
        .arg(&resource)
        .status()
        .expect("Cannot run rc.exe");
    assert!(
        status.success(),
        "Cannot compile native executable identity"
    );
    println!("cargo:rustc-link-arg={}", compiled.display());
}

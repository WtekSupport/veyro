use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let version_path = manifest_dir.join("../version.json");
    println!("cargo:rerun-if-changed={}", version_path.display());

    let version_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&version_path).expect("read version.json"))
            .expect("parse version.json");
    let major = version_json["major"].as_u64().expect("major");
    let minor = version_json["minor"].as_u64().expect("minor");
    let build = version_json["build"].as_u64().expect("build");
    let semver = format!("{major}.{minor}.{build}");
    let display = semver.clone();

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    fs::write(
        out_dir.join("app_version.rs"),
        format!(
            r#"pub const VERSION_DISPLAY: &str = "{display}";
pub const VERSION_SEMVER: &str = "{semver}";
pub const VERSION_MAJOR: u32 = {major};
pub const VERSION_MINOR: u32 = {minor};
pub const VERSION_BUILD: u32 = {build};
"#
        ),
    )
    .expect("write app_version.rs");

    let source = manifest_dir.join("../docs/dictionary/dictionary.default.toml");
    let dest_dir = manifest_dir.join("resources/dictionary");
    let dest = dest_dir.join("dictionary.toml");

    if source.is_file() {
        std::fs::create_dir_all(&dest_dir).expect("create resources/dictionary");
        std::fs::copy(&source, &dest).expect("copy dictionary.default.toml to resources");
        println!("cargo:rerun-if-changed={}", source.display());
    } else if !dest.is_file() {
        panic!(
            "missing default dictionary: {} (expected source at {})",
            dest.display(),
            source.display()
        );
    }

    let skills_source = manifest_dir.join("../docs/skills");
    let skills_dest = manifest_dir.join("resources/skills");
    if skills_source.is_dir() {
        copy_md_files(&skills_source, &skills_dest);
        println!("cargo:rerun-if-changed={}", skills_source.display());
    }

    let legal_source = manifest_dir.join("../docs/legal/third-party-licenses.json");
    let legal_dest_dir = manifest_dir.join("resources/legal");
    let legal_dest = legal_dest_dir.join("third-party-licenses.json");
    if legal_source.is_file() {
        fs::create_dir_all(&legal_dest_dir).expect("create resources/legal");
        fs::copy(&legal_source, &legal_dest).expect("copy third-party-licenses.json");
        println!("cargo:rerun-if-changed={}", legal_source.display());
    } else if !legal_dest.is_file() {
        panic!(
            "missing third-party licenses: {} (expected source at {})",
            legal_dest.display(),
            legal_source.display()
        );
    }

    #[cfg(windows)]
    let windows = tauri_build::WindowsAttributes::new()
        .app_manifest(include_str!("windows/manifest.xml"));

    #[cfg(windows)]
    let attrs = tauri_build::Attributes::new().windows_attributes(windows);

    #[cfg(windows)]
    tauri_build::try_build(attrs).expect("failed to run tauri-build");

    #[cfg(not(windows))]
    tauri_build::build();
}

fn copy_md_files(source_dir: &PathBuf, dest_dir: &PathBuf) {
    std::fs::create_dir_all(dest_dir).expect("create resources/skills");
    let entries = std::fs::read_dir(source_dir).expect("read docs/skills");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        let dest = dest_dir.join(entry.file_name());
        std::fs::copy(&path, &dest).expect("copy skill markdown to resources");
    }
}

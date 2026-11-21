//! `marqdo init` command.

use super::*;

/// `marqdo init`.
pub fn init(dir: Option<&Path>) -> i32 {
    let dir = dir.unwrap_or_else(|| Path::new("."));
    if dir.join(marqdo_package::MANIFEST_FILE).exists() {
        eprintln!(
            "error: `{}` already contains {}",
            dir.display(),
            marqdo_package::MANIFEST_FILE
        );
        return 1;
    }
    let name = dir
        .canonicalize()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "example".to_string());
    if let Err(error) = std::fs::create_dir_all(dir) {
        eprintln!("error: cannot create {}: {error}", dir.display());
        return 1;
    }
    let manifest = marqdo_package::Manifest::template(&name);
    if let Err(error) = std::fs::write(dir.join(marqdo_package::MANIFEST_FILE), manifest.to_yaml())
    {
        eprintln!("error: cannot write manifest: {error}");
        return 1;
    }
    let entry = dir.join("index.mq.md");
    if let Err(error) = std::fs::write(&entry, INIT_TEMPLATE) {
        eprintln!("error: cannot write {}: {error}", entry.display());
        return 1;
    }
    let _ = std::fs::write(dir.join(".gitignore"), ".marqdo/\n");
    println!("Initialized Marqdo project `{name}` in {}", dir.display());
    println!("Next: marqdo check index.mq.md && marqdo run index.mq.md");
    0
}

const INIT_TEMPLATE: &str =
    "## @exec main\n\n`问候 = \"你好，Marqdo\"`\n\n`打印(问候)`\n\n`return 问候`\n";

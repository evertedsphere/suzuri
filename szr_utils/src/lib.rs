use std::path::{Path, PathBuf};

pub fn cargo_workspace_dir() -> PathBuf {
    let output = std::process::Command::new(env!("CARGO"))
        .arg("locate-project")
        .arg("--workspace")
        .arg("--message-format=plain")
        .output()
        .expect("locate project")
        .stdout;
    let cargo_path = Path::new(
        std::str::from_utf8(&output)
            .expect("illegal string for output path")
            .trim(),
    );
    cargo_path.parent().unwrap().to_path_buf()
}

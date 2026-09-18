//! Which server for which language, and where to find it.

use std::path::PathBuf;

pub struct ServerSpec {
    pub program: String,
    pub args: Vec<String>,
}

fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|p| p.join(name))
        .find(|p| p.is_file())
}

/// The language server for `language`, if one is installed. Rust prefers
/// the rustup component so it matches the toolchain.
pub fn find(language: &str) -> Option<ServerSpec> {
    let spec = |program: PathBuf, args: &[&str]| ServerSpec {
        program: program.to_string_lossy().into_owned(),
        args: args.iter().map(|s| s.to_string()).collect(),
    };
    match language {
        "rust" => {
            if let Ok(out) = std::process::Command::new("rustup")
                .args(["which", "rust-analyzer"])
                .output()
            {
                if out.status.success() {
                    let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
                    if p.is_file() {
                        return Some(spec(p, &[]));
                    }
                }
            }
            on_path("rust-analyzer").map(|p| spec(p, &[]))
        }
        "go" => on_path("gopls").map(|p| spec(p, &[])),
        "lean" => on_path("lake").map(|p| spec(p, &["serve"])),
        "typescript" | "javascript" => {
            on_path("typescript-language-server").map(|p| spec(p, &["--stdio"]))
        }
        "python" => on_path("pyright-langserver")
            .map(|p| spec(p, &["--stdio"]))
            .or_else(|| on_path("pylsp").map(|p| spec(p, &[]))),
        _ => None,
    }
}

/// A hint for the status bar when nothing was found.
pub fn install_hint(language: &str) -> Option<&'static str> {
    Some(match language {
        "rust" => "rustup component add rust-analyzer",
        "go" => "go install golang.org/x/tools/gopls@latest",
        "lean" => "install Lean via elan (lake serve)",
        "typescript" | "javascript" => "npm i -g typescript-language-server typescript",
        "python" => "pip install pyright  (or python-lsp-server)",
        _ => return None,
    })
}

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
        // Core languages (spec 010 / Core Languages): the servers people install.
        "c" | "cpp" => on_path("clangd").map(|p| spec(p, &[])),
        "nix" => on_path("nil")
            .map(|p| spec(p, &[]))
            .or_else(|| on_path("nixd").map(|p| spec(p, &[]))),
        "toml" => on_path("taplo").map(|p| spec(p, &["lsp", "stdio"])),
        "typst" => on_path("tinymist").map(|p| spec(p, &["lsp"])),
        "julia" => on_path("julia").map(|p| {
            spec(
                p,
                &[
                    "--startup-file=no",
                    "--history-file=no",
                    "--project=@lsp",
                    "-e",
                    "using LanguageServer; runserver()",
                ],
            )
        }),
        "json" => on_path("vscode-json-language-server").map(|p| spec(p, &["--stdio"])),
        "yaml" => on_path("yaml-language-server").map(|p| spec(p, &["--stdio"])),
        "graphql" => on_path("graphql-lsp").map(|p| spec(p, &["server", "-m", "stream"])),
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
        "c" | "cpp" => "install clangd (llvm)",
        "nix" => "install nil (or nixd)",
        "toml" => "cargo install taplo-cli",
        "typst" => "cargo install tinymist  (or your package manager)",
        "julia" => "julia --project=@lsp -e 'using Pkg; Pkg.add(\"LanguageServer\")'",
        "json" => "npm i -g vscode-langservers-extracted",
        "yaml" => "npm i -g yaml-language-server",
        "graphql" => "npm i -g graphql-language-service-cli",
        _ => return None,
    })
}

//! Default language server configurations.
//!
//! Maps file extensions to language server commands, mirroring the Python
//! `language_servers/` directory configuration.

use super::ServerConfig;

/// Returns a list of default server configurations for common languages.
pub fn default_server_configs() -> Vec<ServerConfig> {
    vec![
        // Rust
        ServerConfig::new("rust-analyzer", vec![], "rust", vec!["rs".to_string()]),
        // Python (Pyright)
        ServerConfig::new(
            "pyright-langserver",
            vec!["--stdio".to_string()],
            "python",
            vec!["py".to_string(), "pyi".to_string()],
        ),
        // TypeScript / JavaScript
        ServerConfig::new(
            "typescript-language-server",
            vec!["--stdio".to_string()],
            "typescript",
            vec![
                "ts".to_string(),
                "tsx".to_string(),
                "js".to_string(),
                "jsx".to_string(),
                "mjs".to_string(),
                "cjs".to_string(),
            ],
        ),
        // Go
        ServerConfig::new(
            "gopls",
            vec!["serve".to_string()],
            "go",
            vec!["go".to_string()],
        ),
        // C / C++
        ServerConfig::new(
            "clangd",
            vec![],
            "cpp",
            vec![
                "c".to_string(),
                "h".to_string(),
                "cpp".to_string(),
                "hpp".to_string(),
                "cc".to_string(),
                "cxx".to_string(),
            ],
        ),
        // Java
        ServerConfig::new("jdtls", vec![], "java", vec!["java".to_string()]),
        // C#
        ServerConfig::new("csharp-ls", vec![], "csharp", vec!["cs".to_string()]),
        // Ruby
        ServerConfig::new("ruby-lsp", vec![], "ruby", vec!["rb".to_string()]),
        // PHP
        ServerConfig::new(
            "intelephense",
            vec!["--stdio".to_string()],
            "php",
            vec!["php".to_string()],
        ),
        // Kotlin
        ServerConfig::new(
            "kotlin-language-server",
            vec![],
            "kotlin",
            vec!["kt".to_string(), "kts".to_string()],
        ),
        // Scala
        ServerConfig::new(
            "metals",
            vec![],
            "scala",
            vec!["scala".to_string(), "sc".to_string()],
        ),
        // Lua
        ServerConfig::new(
            "lua-language-server",
            vec![],
            "lua",
            vec!["lua".to_string()],
        ),
        // Haskell
        ServerConfig::new(
            "haskell-language-server-wrapper",
            vec!["--lsp".to_string()],
            "haskell",
            vec!["hs".to_string()],
        ),
        // Elixir
        ServerConfig::new(
            "elixir-ls",
            vec![],
            "elixir",
            vec!["ex".to_string(), "exs".to_string()],
        ),
        // Dart
        ServerConfig::new(
            "dart",
            vec!["language-server".to_string(), "--protocol=lsp".to_string()],
            "dart",
            vec!["dart".to_string()],
        ),
        // Swift
        ServerConfig::new("sourcekit-lsp", vec![], "swift", vec!["swift".to_string()]),
        // Bash
        ServerConfig::new(
            "bash-language-server",
            vec!["start".to_string()],
            "shellscript",
            vec!["sh".to_string(), "bash".to_string(), "zsh".to_string()],
        ),
        // YAML
        ServerConfig::new(
            "yaml-language-server",
            vec!["--stdio".to_string()],
            "yaml",
            vec!["yml".to_string(), "yaml".to_string()],
        ),
        // Terraform
        ServerConfig::new(
            "terraform-ls",
            vec!["serve".to_string()],
            "terraform",
            vec!["tf".to_string(), "tfvars".to_string()],
        ),
        // Zig
        ServerConfig::new("zls", vec![], "zig", vec!["zig".to_string()]),
        // Markdown
        ServerConfig::new(
            "marksman",
            vec!["server".to_string()],
            "markdown",
            vec!["md".to_string()],
        ),
        // Vue
        ServerConfig::new(
            "vue-language-server",
            vec!["--stdio".to_string()],
            "vue",
            vec!["vue".to_string()],
        ),
        // Svelte
        ServerConfig::new(
            "svelteserver",
            vec!["--stdio".to_string()],
            "svelte",
            vec!["svelte".to_string()],
        ),
        // Astro
        ServerConfig::new(
            "astro-ls",
            vec!["--stdio".to_string()],
            "astro",
            vec!["astro".to_string()],
        ),
        // OCaml
        ServerConfig::new(
            "ocamllsp",
            vec![],
            "ocaml",
            vec!["ml".to_string(), "mli".to_string()],
        ),
        // Gleam
        ServerConfig::new(
            "gleam",
            vec!["lsp".to_string()],
            "gleam",
            vec!["gleam".to_string()],
        ),
        // Clojure
        ServerConfig::new(
            "clojure-lsp",
            vec![],
            "clojure",
            vec![
                "clj".to_string(),
                "cljs".to_string(),
                "cljc".to_string(),
                "edn".to_string(),
            ],
        ),
        // Nix
        ServerConfig::new("nixd", vec![], "nix", vec!["nix".to_string()]),
        // LaTeX
        ServerConfig::new(
            "texlab",
            vec![],
            "latex",
            vec!["tex".to_string(), "bib".to_string()],
        ),
        // Dockerfile
        ServerConfig::new(
            "docker-langserver",
            vec!["--stdio".to_string()],
            "dockerfile",
            vec!["dockerfile".to_string()],
        ),
        // Prisma
        ServerConfig::new(
            "prisma-language-server",
            vec!["--stdio".to_string()],
            "prisma",
            vec!["prisma".to_string()],
        ),
        // F#
        ServerConfig::new(
            "fsautocomplete",
            vec!["--adaptive-lsp-server-enabled".to_string()],
            "fsharp",
            vec!["fs".to_string(), "fsx".to_string(), "fsi".to_string()],
        ),
        // Julia
        ServerConfig::new(
            "julia",
            vec![
                "--startup-file=no".to_string(),
                "-e".to_string(),
                "using LanguageServer; runserver()".to_string(),
            ],
            "julia",
            vec!["jl".to_string()],
        ),
        // Deno (TypeScript variant — only used when Deno project detected)
        ServerConfig::new(
            "deno",
            vec!["lsp".to_string()],
            "deno",
            vec![], // No default extensions — activated by deno.json presence
        ),
        // Typst
        ServerConfig::new("tinymist", vec![], "typst", vec!["typ".to_string()]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_default_configs_non_empty() {
        let configs = default_server_configs();
        assert!(!configs.is_empty());
        assert!(
            configs.len() >= 35,
            "Expected at least 35 server configs, got {}",
            configs.len()
        );
    }

    #[test]
    fn test_no_duplicate_extensions() {
        let configs = default_server_configs();
        let mut all_exts = HashSet::new();
        for config in &configs {
            for ext in &config.extensions {
                assert!(all_exts.insert(ext.clone()), "Duplicate extension: {}", ext);
            }
        }
    }

    #[test]
    fn test_rust_config() {
        let configs = default_server_configs();
        let rust = configs
            .iter()
            .find(|c| c.language_id == "rust")
            .expect("Rust config missing");
        assert_eq!(rust.command, "rust-analyzer");
        assert!(rust.extensions.contains(&"rs".to_string()));
    }

    #[test]
    fn test_all_configs_have_required_fields() {
        let configs = default_server_configs();
        for config in &configs {
            assert!(!config.command.is_empty(), "Empty command");
            assert!(!config.language_id.is_empty(), "Empty language_id");
            // Deno has no default extensions (activated by project detection)
            if config.language_id != "deno" {
                assert!(
                    !config.extensions.is_empty(),
                    "No extensions for {}",
                    config.language_id
                );
            }
        }
    }

    #[test]
    fn test_new_servers_present() {
        let configs = default_server_configs();
        let ids: Vec<&str> = configs.iter().map(|c| c.language_id.as_str()).collect();
        assert!(ids.contains(&"vue"), "Vue LSP missing");
        assert!(ids.contains(&"svelte"), "Svelte LSP missing");
        assert!(ids.contains(&"ocaml"), "OCaml LSP missing");
        assert!(ids.contains(&"gleam"), "Gleam LSP missing");
        assert!(ids.contains(&"clojure"), "Clojure LSP missing");
        assert!(ids.contains(&"nix"), "Nix LSP missing");
        assert!(ids.contains(&"latex"), "LaTeX LSP missing");
        assert!(ids.contains(&"dockerfile"), "Dockerfile LSP missing");
        assert!(ids.contains(&"prisma"), "Prisma LSP missing");
        assert!(ids.contains(&"fsharp"), "F# LSP missing");
        assert!(ids.contains(&"julia"), "Julia LSP missing");
        assert!(ids.contains(&"typst"), "Typst LSP missing");
    }
}

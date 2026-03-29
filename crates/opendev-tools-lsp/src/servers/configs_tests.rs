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

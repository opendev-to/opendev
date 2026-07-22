use super::*;
use clap::Parser;

#[test]
fn chatgpt_login_is_available_only_through_setup() {
    let setup = Cli::try_parse_from(["opendev", "setup"]).unwrap();
    assert!(matches!(setup.command, Some(Commands::Setup)));

    for arguments in [
        ["opendev", "auth", "chatgpt", "login"].as_slice(),
        ["opendev", "auth", "chatgpt", "status"].as_slice(),
        ["opendev", "auth", "chatgpt", "logout"].as_slice(),
    ] {
        assert!(Cli::try_parse_from(arguments).is_err());
    }
}

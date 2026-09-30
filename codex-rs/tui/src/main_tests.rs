use super::*;
use pretty_assertions::assert_eq;

const SOCKET: &str = "unix:///private/tmp/canvastty-test/control.sock";
const SESSION: &str = "01a0effb-7158-7e50-8e58-e5ba98f02dc1";

fn parse(args: &[&str]) -> Result<(Cli, RemoteAppServerEndpoint), clap::Error> {
    TopCli::try_parse_from(args)?.into_remote_args()
}

#[test]
fn remote_endpoint_is_required_and_never_defaults_to_embedded() {
    assert!(parse(&["codex-tui"]).is_err());
    for remote in ["unix://", "unix://relative.sock", "ws://127.0.0.1:4500"] {
        assert!(parse(&["codex-tui", "--remote", remote]).is_err());
    }
    let (_, endpoint) = parse(&["codex-tui", "--remote", SOCKET]).unwrap();
    let RemoteAppServerEndpoint::UnixSocket { socket_path } = endpoint else {
        panic!("frontend must use an explicit Unix socket");
    };
    assert_eq!(
        socket_path.as_path(),
        Path::new("/private/tmp/canvastty-test/control.sock")
    );
}

#[test]
fn explicit_resume_modes_preserve_upstream_controls() {
    let (picker, _) = parse(&[
        "codex-tui",
        "--remote",
        SOCKET,
        "--resume",
        "--resume-show-all",
        "--resume-include-non-interactive",
    ])
    .unwrap();
    assert_eq!(
        (
            picker.resume_picker,
            picker.resume_last,
            picker.resume_session_id,
            picker.resume_show_all,
            picker.resume_include_non_interactive,
            picker.prompt,
        ),
        (true, false, None, true, true, None)
    );
    let (last, _) = parse(&["codex-tui", "--remote", SOCKET, "--resume-last"]).unwrap();
    assert_eq!(
        (last.resume_picker, last.resume_last, last.resume_session_id),
        (false, true, None)
    );
    let (session, _) = parse(&[
        "codex-tui",
        "--remote",
        SOCKET,
        "--resume-session-id",
        SESSION,
    ])
    .unwrap();
    assert_eq!(
        (
            session.resume_picker,
            session.resume_last,
            session.resume_session_id,
            session.prompt,
        ),
        (false, false, Some(SESSION.to_string()), None)
    );
}

#[test]
fn resume_controls_reject_ambiguous_or_incomplete_arguments() {
    for flags in [
        vec!["--resume", "--resume-last"],
        vec!["--resume", "--resume-session-id", SESSION],
        vec!["--resume-last", "--resume-session-id", SESSION],
        vec!["--resume-show-all"],
        vec!["--resume-include-non-interactive"],
        vec!["--resume-session-id", "not-a-session"],
    ] {
        let mut args = vec!["codex-tui", "--remote", SOCKET];
        args.extend(flags);
        assert!(parse(&args).is_err(), "unexpectedly accepted {args:?}");
    }
}

#[test]
fn positional_resume_and_fork_cannot_become_submitted_prompts() {
    for reserved in ["resume", "fork"] {
        assert!(parse(&["codex-tui", "--remote", SOCKET, reserved]).is_err());
        assert!(parse(&["codex-tui", "--remote", SOCKET, reserved, SESSION]).is_err());
    }
}

#[test]
fn remote_only_frontend_rejects_no_daemon() {
    assert!(parse(&["codex-tui", "--remote", SOCKET, "--no-daemon"]).is_err());
}

#[test]
fn owned_local_server_requires_an_explicit_launcher_marker() {
    let (ordinary, _) = parse(&["codex-tui", "--remote", SOCKET]).unwrap();
    let (owned, _) = parse(&["codex-tui", "--remote", SOCKET, "--owned-local-server"]).unwrap();
    assert_eq!(
        (ordinary.owned_local_server, owned.owned_local_server),
        (false, true)
    );
    assert!(parse(&["codex-tui", "--owned-local-server"]).is_err());
}

#[test]
fn ordinary_stock_flags_and_explicit_initial_prompt_are_preserved() {
    let (cli, _) = parse(&[
        "codex-tui",
        "--remote",
        SOCKET,
        "--no-alt-screen",
        "--strict-config",
        "-c",
        "model_reasoning_effort=\"high\"",
        "Привет, мир",
    ])
    .unwrap();
    assert_eq!(
        (
            cli.no_alt_screen,
            cli.strict_config,
            cli.config_overrides.raw_overrides,
            cli.prompt,
        ),
        (
            true,
            true,
            vec!["model_reasoning_effort=\"high\"".to_string()],
            Some("Привет, мир".to_string()),
        )
    );
}

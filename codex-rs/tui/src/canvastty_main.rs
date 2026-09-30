use clap::ArgGroup;
use clap::CommandFactory;
use clap::Parser;
use clap::error::ErrorKind;
use codex_arg0::Arg0DispatchPaths;
use codex_arg0::arg0_dispatch_or_else;
use codex_config::LoaderOverrides;
use codex_tui::Cli;
use codex_tui::ExitReason;
use codex_tui::RemoteAppServerEndpoint;
use codex_tui::resolve_remote_addr;
use codex_tui::run_main;
use codex_utils_cli::CliConfigOverrides;
use std::io::Write;
use std::path::Path;
use supports_color::Stream;

#[derive(Parser, Debug)]
#[command(name = "canvastty-codex-tui")]
#[command(group(ArgGroup::new("resume_mode").args([
    "resume_picker",
    "resume_last",
    "resume_session_id",
])))]
struct TopCli {
    #[clap(flatten)]
    config_overrides: CliConfigOverrides,

    #[clap(flatten)]
    inner: Cli,

    /// Connect to the official backend on an explicit absolute Unix socket.
    #[arg(long, value_parser = absolute_unix_remote)]
    remote: String,

    /// Preserve the selected launch permissions when the launcher owns this backend.
    #[arg(long)]
    owned_local_server: bool,

    /// Choose a previous session in the upstream resume picker.
    #[arg(long = "resume")]
    resume_picker: bool,

    /// Resume the last session without opening the picker.
    #[arg(long)]
    resume_last: bool,

    /// Resume this session UUID without treating it as an initial prompt.
    #[arg(long, value_parser = session_uuid)]
    resume_session_id: Option<String>,

    #[arg(long, requires = "resume_mode")]
    resume_show_all: bool,

    #[arg(long, requires = "resume_mode")]
    resume_include_non_interactive: bool,
}

fn absolute_unix_remote(value: &str) -> Result<String, String> {
    let socket = value
        .strip_prefix("unix://")
        .filter(|socket| Path::new(socket).is_absolute())
        .ok_or_else(|| {
            "--remote requires unix:// followed by an absolute socket path".to_string()
        })?;
    if socket.contains('\0') {
        return Err("--remote socket path must not contain a NUL byte".to_string());
    }
    Ok(value.to_string())
}

fn session_uuid(value: &str) -> Result<String, String> {
    uuid::Uuid::parse_str(value)
        .map(|id| id.to_string())
        .map_err(|err| format!("invalid session UUID: {err}"))
}

impl TopCli {
    fn into_remote_args(self) -> Result<(Cli, RemoteAppServerEndpoint), clap::Error> {
        if self.inner.no_daemon {
            return Err(Self::command().error(
                ErrorKind::ArgumentConflict,
                "--no-daemon cannot be used with this remote-only frontend",
            ));
        }
        if matches!(self.inner.prompt.as_deref(), Some("resume" | "fork")) {
            return Err(Self::command().error(
                ErrorKind::InvalidValue,
                "resume/fork are not positional commands; use explicit --resume flags",
            ));
        }
        let endpoint = resolve_remote_addr(&self.remote)
            .map_err(|err| Self::command().error(ErrorKind::InvalidValue, err.to_string()))?;
        let mut inner = self.inner;
        inner
            .config_overrides
            .raw_overrides
            .splice(0..0, self.config_overrides.raw_overrides);
        inner.resume_picker = self.resume_picker;
        inner.resume_last = self.resume_last;
        inner.resume_session_id = self.resume_session_id;
        inner.resume_show_all = self.resume_show_all;
        inner.resume_include_non_interactive = self.resume_include_non_interactive;
        inner.owned_local_server = self.owned_local_server;
        Ok((inner, endpoint))
    }
}

fn main() -> anyhow::Result<()> {
    codex_build_info::initialize!();
    arg0_dispatch_or_else(|arg0_paths: Arg0DispatchPaths| async move {
        let (inner, endpoint) = TopCli::parse()
            .into_remote_args()
            .unwrap_or_else(|err| err.exit());
        let exit_info = run_main(
            inner,
            arg0_paths,
            LoaderOverrides::default(),
            Some(endpoint),
        )
        .await?;
        let is_fatal = match &exit_info.exit_reason {
            ExitReason::Fatal(message) => {
                eprintln!("ERROR: {message}");
                true
            }
            ExitReason::UserRequested
            | ExitReason::Archived(_)
            | ExitReason::TurnInterrupted
            | ExitReason::ThreadRemoved => false,
        };

        let color_enabled = supports_color::on(Stream::Stdout).is_some();
        for line in exit_info.format_exit_messages(color_enabled) {
            println!("{line}");
        }
        if is_fatal {
            std::io::stdout().flush()?;
            std::process::exit(1);
        }
        Ok(())
    })
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;

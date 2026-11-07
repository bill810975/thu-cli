use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};
use thu_cli::{
    app, auth, credential, learn,
    session::{self, Session},
    thu,
};
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(
    name = "thu-cli",
    version,
    about = "Native Rust Tsinghua campus and learning services",
    long_about = "Native Rust Tsinghua campus and learning services. Use `thu-cli services` to discover operations, or `thu-cli describe thu OPERATION` to inspect arguments. No live login is attempted without an explicit command."
)]
struct Cli {
    /// Private cookie store path (default: platform user config directory)
    #[arg(long, global = true)]
    session: Option<PathBuf>,
    /// Compact machine-readable JSON instead of pretty-printed JSON
    #[arg(long, global = true)]
    compact: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// List all available service operations and argument descriptions
    Services {
        #[arg(value_parser=["thu","learn","app"])]
        service: Option<String>,
    },
    /// Describe a service operation without making a request
    Describe {
        #[arg(value_parser=["thu","learn","app"])]
        service: String,
        operation: String,
    },
    /// Log in using native identity protocol; password is prompted invisibly
    Login {
        #[arg(long)]
        username: String,
        /// Destination session to initialize
        #[arg(long, default_value = "info")]
        service: String,
        /// Optional protected password file for a one-off login
        #[arg(long)]
        password_file: Option<PathBuf>,
        /// Reuse the current session and refresh it only if authentication is required
        #[arg(long)]
        refresh: bool,
        /// Stable caller-supplied device fingerprint
        #[arg(long, default_value = "thu-cli-native")]
        fingerprint: String,
        #[arg(long)]
        two_factor_method: Option<String>,
        /// Register this CLI fingerprint as a trusted identity device after 2FA
        #[arg(long)]
        trust_device: bool,
        /// Use WebVPN instead of direct campus-network access for Info login
        #[arg(long)]
        webvpn: bool,
        /// Fail rather than prompt when CAPTCHA/MFA requires interaction
        #[arg(long)]
        non_interactive: bool,
    },
    /// Manage the private local JSON credential used for unattended renewal
    Credential {
        #[command(subcommand)]
        command: CredentialCommand,
    },
    /// Bootstrap a destination system through the upstream SSO roaming protocol
    Roam {
        #[arg(long, default_value = "default")]
        policy: String,
        #[arg(long)]
        payload: String,
        /// Optional username when destination requires credential reauthentication
        #[arg(long)]
        username: Option<String>,
        #[arg(long, default_value = "thu-cli-native")]
        fingerprint: String,
    },
    /// Call a campus service. See `services thu` and `describe thu OPERATION`.
    Thu(Call),
    /// Call a Learn service. See `services learn` and `describe learn OPERATION`.
    Learn(Call),
    /// App-only washer status, schedule transfer and ICS export
    App(Call),
    /// Inspect local cookie metadata; does not claim server authentication
    Status,
    /// Import a private Netscape cookie export after interactive browser login
    ImportCookies {
        file: PathBuf,
        #[arg(long)]
        confirm: bool,
    },
    /// Clear local session cookies only (does not revoke server-side sessions)
    Logout {
        #[arg(long)]
        confirm: bool,
    },
}
#[derive(Subcommand)]
enum CredentialCommand {
    /// Save or replace the campus password after a hidden prompt
    Set {
        #[arg(long)]
        username: String,
        #[arg(long)]
        confirm: bool,
    },
    /// Remove the saved campus password
    Delete {
        #[arg(long)]
        username: String,
        #[arg(long)]
        confirm: bool,
    },
    /// Check whether a campus password is available without printing it
    Status {
        #[arg(long)]
        username: String,
    },
}
#[derive(clap::Args)]
struct Call {
    operation: String,
    /// Username for destination SSO; password is read locally or prompted invisibly
    #[arg(long)]
    username: Option<String>,
    /// Skip automatic destination SSO only when a valid target session already exists
    #[arg(long)]
    no_roam: bool,
    /// Do not refresh an expired trusted-device session automatically
    #[arg(long)]
    no_auto_refresh: bool,
    /// JSON object containing operation arguments
    #[arg(long, default_value = "{}", conflicts_with = "args_file")]
    args: String,
    /// Read JSON arguments from file; use for sensitive tokens to avoid shell history
    #[arg(long)]
    args_file: Option<PathBuf>,
    /// Authorize this state-changing operation
    #[arg(long)]
    confirm: bool,
    /// Validate operation and required argument presence without network access
    #[arg(long)]
    dry_run: bool,
}
fn operations(service: &str) -> Value {
    match service {
        "thu" => thu::operations(),
        "app" => app::operations(),
        _ => learn::operations(),
    }
}
fn operation(service: &str, name: &str) -> Result<Value> {
    operations(service)
        .as_array()
        .context("Invalid built-in operation catalog")?
        .iter()
        .find(|v| v["name"] == name)
        .cloned()
        .with_context(|| {
            format!("Unknown {service} operation '{name}'; run `thu-cli services {service}`")
        })
}
fn validate(op: &Value, args: &Value, confirmed: bool, dry_run: bool) -> Result<()> {
    if !args.is_object() {
        bail!("--args must be a JSON object");
    }
    if let Some(required) = op["required"].as_array() {
        for key in required.iter().filter_map(Value::as_str) {
            if args.get(key).is_none_or(Value::is_null) {
                bail!("Missing required argument: {key}");
            }
        }
    }
    // Fail closed for malformed metadata.
    if op["mutating"].as_bool().unwrap_or(true) && !confirmed && !dry_run {
        bail!("This operation changes server state. Review arguments, then repeat with --confirm. Use --dry-run to validate without sending.");
    }
    Ok(())
}
fn password(file: Option<PathBuf>) -> Result<Zeroizing<String>> {
    if let Some(path) = file {
        password_from_file(&path)
    } else {
        Ok(Zeroizing::new(rpassword::prompt_password("Password: ")?))
    }
}
fn password_from_file(path: &PathBuf) -> Result<Zeroizing<String>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
            bail!("Password file must have mode 600 or stricter");
        }
    }
    Ok(Zeroizing::new(
        fs::read_to_string(path)?
            .trim_end_matches(['\r', '\n'])
            .to_owned(),
    ))
}

fn local_password(session: &Session, username: &str) -> Result<Option<Zeroizing<String>>> {
    credential::get(&session.credential_path(), username)
}

fn local_or_prompted_password(session: &Session, username: &str) -> Result<Zeroizing<String>> {
    if let Some(password) = local_password(session, username)? {
        return Ok(password);
    }
    password(None)
}

async fn execute_service(
    session: &Session,
    service: &str,
    operation: &str,
    args: &Value,
) -> Result<Value> {
    if service == "app" {
        app::execute(&session.client, operation, args).await
    } else if service == "thu" {
        thu::execute(session, operation, args).await
    } else {
        learn::execute(&session.client, operation, args).await
    }
}

async fn refresh_session(session: &Session) -> Result<()> {
    let profile = session.load_profile()?.ok_or_else(|| {
        anyhow::anyhow!(
            "automatic session refresh is unavailable because this profile has no saved login metadata; rerun login once"
        )
    })?;
    let mut options = auth::LoginOptions {
        username: profile.username.clone(),
        password: String::new(),
        fingerprint: profile.fingerprint.clone(),
        service: profile.service.clone(),
        two_factor_method: None,
        interactive: false,
        trust_device: true,
        use_webvpn: profile.use_webvpn,
    };
    match auth::login(&session.auth_client, &options).await {
        Ok(_) => Ok(()),
        Err(error) if auth::is_password_required(&error) => {
            let secret = local_password(session, &profile.username)?.ok_or_else(|| {
                error.context(format!(
                    "trusted-device renewal requires the local credential; run `thu-cli --session {} credential set --username {} --confirm`",
                    session.path().display(),
                    profile.username
                ))
            })?;
            options.password = secret.to_string();
            auth::login(&session.auth_client, &options)
                .await
                .map(|_| ())
        }
        Err(error) => Err(error),
    }
}

async fn roam_for_call(session: &Session, call: &Call, policy: &str, payload: &str) -> Result<()> {
    let credentials = if let Some(user) = &call.username {
        Some(auth::LoginOptions {
            username: user.clone(),
            password: local_or_prompted_password(session, user)?.to_string(),
            fingerprint: "thu-cli-native".into(),
            service: "info".into(),
            two_factor_method: None,
            interactive: true,
            trust_device: false,
            use_webvpn: false,
        })
    } else {
        None
    };
    let csrf = session.cookie_value("info.tsinghua.edu.cn", "XSRF-TOKEN")?;
    auth::roam_with_csrf(
        &session.auth_client,
        &auth::RoamOptions {
            policy: policy.to_owned(),
            payload: payload.to_owned(),
            credentials,
        },
        csrf.as_deref(),
    )
    .await
    .map(|_| ())
}

async fn execute_service_with_recovery(
    session: &Session,
    service: &str,
    call: &Call,
    op: &Value,
    args: &Value,
) -> Result<Value> {
    let mutating = op["mutating"].as_bool().unwrap_or(true);
    let auto_refresh = !call.no_auto_refresh;
    let roam = if service == "thu" && !call.no_roam {
        thu::roaming(&call.operation, args)
    } else {
        None
    };

    // Read-only campus requests try the direct endpoint first. This keeps
    // intranet calls fast and avoids requiring a WebVPN session on campus.
    if service == "thu" && !mutating {
        let first_error = match execute_service(session, service, &call.operation, args).await {
            Ok(result) => return Ok(result),
            Err(error) => error,
        };
        let mut refreshed = false;
        if auto_refresh && auth::is_auth_failure(&first_error) {
            refresh_session(session).await?;
            refreshed = true;
            if let Ok(result) = execute_service(session, service, &call.operation, args).await {
                return Ok(result);
            }
        }
        let Some((policy, payload)) = roam else {
            if refreshed {
                return execute_service(session, service, &call.operation, args).await;
            }
            return Err(first_error);
        };

        if let Err(error) = roam_for_call(session, call, policy, &payload).await {
            if auto_refresh && !refreshed && auth::is_auth_failure(&error) {
                refresh_session(session).await?;
                refreshed = true;
                roam_for_call(session, call, policy, &payload).await?;
            } else {
                return Err(error
                    .context("direct request failed and destination-session roaming also failed"));
            }
        }
        match execute_service(session, service, &call.operation, args).await {
            Err(error) if auto_refresh && !refreshed && auth::is_auth_failure(&error) => {
                refresh_session(session).await?;
                execute_service(session, service, &call.operation, args).await
            }
            result => result,
        }
    } else if service == "learn" && !mutating {
        match execute_service(session, service, &call.operation, args).await {
            Err(error) if auto_refresh && auth::is_auth_failure(&error) => {
                refresh_session(session).await?;
                execute_service(session, service, &call.operation, args).await
            }
            result => result,
        }
    } else {
        // Establish the destination before a state-changing request. A refresh
        // may be retried only while roaming, before the operation itself runs.
        if let Some((policy, payload)) = roam {
            if let Err(error) = roam_for_call(session, call, policy, &payload).await {
                if auto_refresh && auth::is_auth_failure(&error) {
                    refresh_session(session).await?;
                    roam_for_call(session, call, policy, &payload).await?;
                } else {
                    return Err(error);
                }
            }
        }
        let result = execute_service(session, service, &call.operation, args).await;
        match result {
            Err(error) if mutating && auth::is_auth_failure(&error) => Err(error.context(
                "authentication expired during a state-changing operation; it was not retried because its outcome may be uncertain",
            )),
            result => result,
        }
    }
}

async fn run(cli: &Cli) -> Result<Value> {
    match &cli.command {
        Command::Services { service } => {
            return Ok(match service {
                Some(s) => operations(s),
                None => {
                    json!({"thu":thu::operations(),"learn":learn::operations(),"app":app::operations()})
                }
            })
        }
        Command::Describe {
            service,
            operation: name,
        } => return operation(service, name),
        _ => {}
    }
    if let Command::Thu(call) | Command::Learn(call) | Command::App(call) = &cli.command {
        let service = match &cli.command {
            Command::Thu(_) => "thu",
            Command::App(_) => "app",
            _ => "learn",
        };
        let args: Value = if let Some(file) = &call.args_file {
            serde_json::from_str(&fs::read_to_string(file)?)?
        } else {
            serde_json::from_str(&call.args)?
        };
        check_sensitive_args_file(call.args_file.as_ref(), &args)?;
        let op = operation(service, &call.operation)?;
        validate(&op, &args, call.confirm, call.dry_run)?;
        if call.dry_run {
            return Ok(
                json!({"dry_run":true,"network_request_sent":false,"operation":op,"argument_keys":args.as_object().unwrap().keys().collect::<Vec<_>>()}),
            );
        }
        let session = Session::open(
            cli.session
                .clone()
                .map(Ok)
                .unwrap_or_else(session::default_path)?,
        )?;
        let result = execute_service_with_recovery(&session, service, call, &op, &args).await;
        let saved = session.save();
        if let Err(e) = saved {
            return Err(e.context("Session checkpoint failed; the operation may already have executed. Do not blindly retry a mutation"));
        }
        return result;
    }
    let session = Session::open(
        cli.session
            .clone()
            .map(Ok)
            .unwrap_or_else(session::default_path)?,
    )?;
    let result = match &cli.command {
        Command::Status => session.status(),
        Command::ImportCookies { file, confirm } => {
            if !confirm {
                bail!("Use --confirm to replace the local session with imported cookies");
            }
            let count = session.import_netscape(file)?;
            session.clear_profile()?;
            session.remove_private_sidecar("wsbx-auth")?;
            session.remove_private_sidecar("sports-auth")?;
            Ok(json!({"imported_cookies":count}))
        }
        Command::Logout { confirm } => {
            if !confirm {
                bail!("Use --confirm to clear local session cookies");
            }
            session.clear()?;
            Ok(json!({"local_session_cleared":true,"server_session_revoked":false}))
        }
        Command::Login {
            username,
            service,
            password_file,
            refresh,
            fingerprint,
            two_factor_method,
            trust_device,
            webvpn,
            non_interactive,
        } => {
            if !refresh {
                session.clear()?;
            }
            let mut options = auth::LoginOptions {
                username: username.clone(),
                password: String::new(),
                fingerprint: fingerprint.clone(),
                service: service.clone(),
                two_factor_method: two_factor_method.clone(),
                interactive: !*non_interactive,
                trust_device: *trust_device,
                use_webvpn: *webvpn,
            };
            let login_result = match auth::login(&session.auth_client, &options).await {
                Ok(result) => Ok(result),
                Err(error) if auth::is_password_required(&error) => {
                    let secret = if let Some(path) = password_file {
                        password_from_file(path)?
                    } else if let Some(password) = local_password(&session, username)? {
                        password
                    } else if !*non_interactive {
                        password(None)?
                    } else {
                        return Err(error.context(format!(
                            "no local credential is saved for this account; run `thu-cli --session {} credential set --username {} --confirm`",
                            session.path().display(),
                            username
                        )));
                    };
                    options.password = secret.to_string();
                    auth::login(&session.auth_client, &options).await
                }
                Err(error) => Err(error),
            };
            let result = login_result?;
            session.save_profile(&session::AuthProfile {
                username: username.clone(),
                fingerprint: fingerprint.clone(),
                service: service.clone(),
                use_webvpn: *webvpn,
            })?;
            Ok(result)
        }
        Command::Credential { command } => match command {
            CredentialCommand::Set { username, confirm } => {
                if !confirm {
                    bail!("Use --confirm to save or replace the local credential");
                }
                let path = session.credential_path();
                credential::set_interactive(&path, username)?;
                Ok(json!({
                    "credential_saved":true,
                    "backend":"local JSON",
                    "path":path,
                    "mode":"600"
                }))
            }
            CredentialCommand::Delete { username, confirm } => {
                if !confirm {
                    bail!("Use --confirm to remove the local credential");
                }
                let path = session.credential_path();
                let deleted = credential::delete(&path, username)?;
                Ok(json!({"credential_deleted":deleted,"backend":"local JSON","path":path}))
            }
            CredentialCommand::Status { username } => {
                let path = session.credential_path();
                let available = credential::get(&path, username)?.is_some();
                Ok(json!({
                    "credential_available":available,
                    "backend":"local JSON",
                    "path":path,
                    "mode":"600"
                }))
            }
        },
        Command::Roam {
            policy,
            payload,
            username,
            fingerprint,
        } => {
            let credentials = if let Some(user) = username {
                Some(auth::LoginOptions {
                    username: user.clone(),
                    password: local_or_prompted_password(&session, user)?.to_string(),
                    fingerprint: fingerprint.clone(),
                    service: "info".into(),
                    two_factor_method: None,
                    interactive: true,
                    trust_device: false,
                    use_webvpn: false,
                })
            } else {
                None
            };
            let csrf = session.cookie_value("info.tsinghua.edu.cn", "XSRF-TOKEN")?;
            auth::roam_with_csrf(
                &session.auth_client,
                &auth::RoamOptions {
                    policy: policy.clone(),
                    payload: payload.clone(),
                    credentials,
                },
                csrf.as_deref(),
            )
            .await
            .map(|_| json!({"roaming_completed":true,"policy":policy}))
        }
        _ => unreachable!(),
    };
    session
        .save()
        .context("Could not save private session checkpoint")?;
    result
}
#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match run(&cli).await {
        Ok(v) => println!(
            "{}",
            if cli.compact {
                v.to_string()
            } else {
                serde_json::to_string_pretty(&v).unwrap()
            }
        ),
        Err(e) => {
            eprintln!(
                "{}",
                json!({"error":redact_error(&e.to_string()),"ok":false})
            );
            std::process::exit(1);
        }
    }
}
fn redact_error(s: &str) -> String {
    regex::Regex::new(r"https?://[^\s)]+")
        .unwrap()
        .replace_all(s, "[URL redacted]")
        .into_owned()
}

fn check_sensitive_args_file(path: Option<&PathBuf>, args: &Value) -> Result<()> {
    let sensitive = args.as_object().is_some_and(|m| {
        m.keys().any(|k| {
            let k = k.to_ascii_lowercase();
            k.contains("token")
                || k.contains("password")
                || k.contains("secret")
                || k.contains("cookie")
        })
    });
    if sensitive {
        if let Some(p) = path {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if fs::metadata(p)?.permissions().mode() & 0o077 != 0 {
                    bail!("Arguments file contains credentials and must have mode 600 or stricter");
                }
            }
        } else {
            eprintln!("Warning: secret arguments in command-line JSON can appear in shell history/process listings; prefer --args-file with mode 600.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mutation_requires_confirmation() {
        let op = json!({"mutating":true,"required":["id"]});
        assert!(validate(&op, &json!({"id":"a"}), false, false).is_err());
        assert!(validate(&op, &json!({"id":"a"}), false, true).is_ok());
        assert!(validate(&op, &json!({}), true, false).is_err());
    }
    #[test]
    fn invalid_metadata_fails_closed() {
        assert!(validate(&json!({}), &json!({}), false, false).is_err());
    }
}

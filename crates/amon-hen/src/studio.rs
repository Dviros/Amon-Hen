use super::*;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::style::force_color_output;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Gauge, List, ListItem, Paragraph, Row, Table, Tabs, Wrap,
};
use ratatui::{Frame, Terminal};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};

const MENU: [&str; 18] = [
    "Run / re-run",
    "Cancel job",
    "Edit prompt",
    "Social login",
    "Auth status",
    "Linear status",
    "Deliver Linear",
    "Save profile",
    "Load profile",
    "Tag local file",
    "Run command",
    "Settings",
    "Agents",
    "Capabilities",
    "Refresh capabilities",
    "Linear",
    "Help",
    "Quit",
];

const PANES: [Pane; 6] = [
    Pane::Menu,
    Pane::Settings,
    Pane::Agents,
    Pane::Capabilities,
    Pane::Linear,
    Pane::Results,
];

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Pane {
    Menu,
    Settings,
    Agents,
    Capabilities,
    Linear,
    Results,
}

#[derive(Debug, Clone)]
enum InputMode {
    Prompt,
    File,
    Command,
    LinearIssue,
    LinearQuery,
    LinearProject,
    LinearEpic,
    LinearTeam,
    LinearState,
    LinearMedia,
    CodexModel,
    ClaudeModel,
    GeminiModel,
    CodexConfig,
    CodexProfile,
    ClaudeMcpConfig,
    ClaudeAllowedTools,
    ClaudeDisallowedTools,
    ClaudeTools,
    ClaudeAgent,
    ClaudeAgentsJson,
    ClaudePluginDir,
    GeminiSettings,
    GeminiToolsProfile,
    GeminiAllowedMcp,
    GeminiPolicy,
    GeminiAdminPolicy,
    SaveProfile,
    LoadProfile,
}

struct StudioState {
    resolved: ResolvedArgs,
    prompt: String,
    menu_index: usize,
    focus: Pane,
    pane_order: Vec<Pane>,
    setting_index: usize,
    capability_index: usize,
    linear_index: usize,
    result_index: usize,
    last_result: Option<AmonHenResult>,
    last_linear_result: Option<String>,
    last_auth_result: Option<String>,
    last_capability_result: Option<String>,
    run_job: Option<StudioRunJob>,
    run_events: Vec<String>,
    profile_name: String,
    profile_path: PathBuf,
    profile_names: Vec<String>,
    provider_status: HashMap<String, String>,
    provider_detail: HashMap<String, String>,
    live_token_usage: HashMap<String, TokenUsage>,
    live_tool_counts: HashMap<String, usize>,
    live_sub_agents: HashMap<String, HashSet<String>>,
    status: String,
    input_mode: Option<InputMode>,
    input_buffer: String,
    show_help: bool,
    exit_armed_until: Option<Instant>,
}

struct StudioRunJob {
    rx: Receiver<StudioJobMessage>,
    started: Instant,
    cancel: Arc<AtomicBool>,
    kind: StudioJobKind,
}

enum StudioJobMessage {
    Progress(ProgressEvent),
    Log(String),
    Finished(Box<AmonHenResult>),
    ExternalFinished(Result<StudioJobOutcome, String>),
    Cancelled(String),
    Failed(String),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum StudioJobKind {
    AmonHen,
    SocialLogin,
    AuthStatus,
    CapabilitiesStatus,
    LinearStatus,
    LinearDeliver,
}

impl StudioJobKind {
    fn label(self) -> &'static str {
        match self {
            Self::AmonHen => "Amon Hen run",
            Self::SocialLogin => "Social login",
            Self::AuthStatus => "Auth status",
            Self::CapabilitiesStatus => "Provider capabilities",
            Self::LinearStatus => "Linear status",
            Self::LinearDeliver => "Linear delivery",
        }
    }

    fn running_status(self) -> &'static str {
        match self {
            Self::AmonHen => "Amon Hen running inside Studio",
            Self::SocialLogin => "Social login running inside Studio",
            Self::AuthStatus => "Refreshing auth status inside Studio",
            Self::CapabilitiesStatus => "Refreshing provider capabilities inside Studio",
            Self::LinearStatus => "Refreshing Linear status inside Studio",
            Self::LinearDeliver => "Delivering Linear work inside Studio",
        }
    }
}

struct StudioJobOutcome {
    status: String,
    focus: Pane,
    auth_result: Option<String>,
    capability_result: Option<String>,
    linear_result: Option<String>,
}

enum StudioAction {
    None,
    RunAmonHen,
    CancelJob,
    SocialLogin,
    AuthStatus,
    CapabilitiesStatus,
    LinearStatus,
    LinearDeliver,
    Quit,
}

#[cfg(test)]
fn dashboard_job_kind(action: &StudioAction) -> Option<StudioJobKind> {
    match action {
        StudioAction::RunAmonHen => Some(StudioJobKind::AmonHen),
        StudioAction::SocialLogin => Some(StudioJobKind::SocialLogin),
        StudioAction::AuthStatus => Some(StudioJobKind::AuthStatus),
        StudioAction::CapabilitiesStatus => Some(StudioJobKind::CapabilitiesStatus),
        StudioAction::LinearStatus => Some(StudioJobKind::LinearStatus),
        StudioAction::LinearDeliver => Some(StudioJobKind::LinearDeliver),
        StudioAction::None | StudioAction::CancelJob | StudioAction::Quit => None,
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct StudioProfilesFile {
    profiles: HashMap<String, StudioProfile>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct StudioProfile {
    prompt: String,
    members: Vec<String>,
    handoff: bool,
    lead: Option<String>,
    planner: Option<String>,
    summarizer: String,
    iterations: usize,
    team_work: usize,
    codex_sub_agents: Option<usize>,
    claude_sub_agents: Option<usize>,
    gemini_sub_agents: Option<usize>,
    codex_model: Option<String>,
    claude_model: Option<String>,
    gemini_model: Option<String>,
    codex_effort: Option<String>,
    claude_effort: Option<String>,
    gemini_effort: Option<String>,
    codex_auth: String,
    claude_auth: String,
    gemini_auth: String,
    codex_sandbox: String,
    claude_permission_mode: String,
    codex_capabilities: String,
    codex_config: Vec<String>,
    codex_mcp_profile: Option<String>,
    claude_capabilities: String,
    claude_mcp_config: Vec<String>,
    claude_allowed_tools: Vec<String>,
    claude_disallowed_tools: Vec<String>,
    claude_tools: Vec<String>,
    claude_agent: Option<String>,
    claude_agents_json: Option<String>,
    claude_plugin_dir: Vec<String>,
    claude_strict_mcp_config: bool,
    claude_disable_slash_commands: bool,
    gemini_capabilities: String,
    gemini_settings: Option<String>,
    gemini_tools_profile: Vec<String>,
    gemini_allowed_mcp_servers: Vec<String>,
    gemini_policy: Vec<String>,
    gemini_admin_policy: Vec<String>,
    deliver_linear: bool,
    linear_watch: bool,
    linear_until_complete: bool,
    linear_auth: String,
    linear_issue: Vec<String>,
    linear_query: Option<String>,
    linear_project: Vec<String>,
    linear_epic: Vec<String>,
    linear_team: Option<String>,
    linear_state: Option<String>,
    linear_assignee: Option<String>,
    linear_limit: usize,
    linear_endpoint: Option<String>,
    linear_api_key_env: String,
    linear_oauth_token_env: String,
    linear_completion_gate: String,
    linear_review_state: Option<String>,
    linear_ci_timeout: u64,
    linear_ci_poll_interval: u64,
    linear_workspace_strategy: String,
    linear_poll_interval: u64,
    linear_max_polls: Option<usize>,
    linear_max_concurrency: usize,
    linear_max_attempts: usize,
    linear_retry_base: u64,
    linear_state_file: Option<PathBuf>,
    linear_workspace_root: Option<PathBuf>,
    linear_observability_dir: Option<PathBuf>,
    linear_workflow_file: Option<PathBuf>,
    no_linear_comments: bool,
    linear_update_review_state: bool,
    linear_attach_media: Vec<String>,
    linear_attachment_title: Option<String>,
    delivery_phases: Vec<String>,
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self, String> {
        enable_raw_mode().map_err(|error| format!("Failed to enable raw mode: {error}"))?;
        execute!(
            io::stderr(),
            EnterAlternateScreen,
            Hide,
            Clear(ClearType::All)
        )
        .map_err(|error| format!("Failed to enter Studio screen: {error}"))?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(
            io::stderr(),
            Show,
            LeaveAlternateScreen,
            Clear(ClearType::All)
        );
        let _ = disable_raw_mode();
    }
}

pub(super) fn run_studio(resolved: &ResolvedArgs) -> i32 {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        println!("{}", render_noninteractive_studio_snapshot(resolved));
        return 0;
    }

    let profile_path = studio_profile_path(&resolved.cwd);
    let profile_names = studio_profile_names(&profile_path).unwrap_or_default();
    let mut state = StudioState {
        resolved: resolved.clone(),
        prompt: resolved.prompt.trim().to_string(),
        menu_index: 0,
        focus: Pane::Menu,
        pane_order: PANES.to_vec(),
        setting_index: 0,
        capability_index: 0,
        linear_index: 0,
        result_index: 0,
        last_result: None,
        last_linear_result: None,
        last_auth_result: None,
        last_capability_result: None,
        run_job: None,
        run_events: Vec::new(),
        profile_name: "default".to_string(),
        profile_path,
        profile_names,
        provider_status: HashMap::new(),
        provider_detail: HashMap::new(),
        live_token_usage: HashMap::new(),
        live_tool_counts: HashMap::new(),
        live_sub_agents: HashMap::new(),
        status: "Ready".to_string(),
        input_mode: None,
        input_buffer: String::new(),
        show_help: false,
        exit_armed_until: None,
    };

    configure_studio_color(&state.resolved.raw);

    let guard = match TerminalGuard::enter() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };

    loop {
        drain_studio_job(&mut state);
        if let Err(error) = draw(&state) {
            drop(guard);
            eprintln!("{error}");
            return 1;
        }
        let poll_timeout = if state.run_job.is_some() {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(250)
        };
        let has_event = match event::poll(poll_timeout) {
            Ok(has_event) => has_event,
            Err(error) => {
                drop(guard);
                eprintln!("Failed to poll Studio input: {error}");
                return 1;
            }
        };
        if !has_event {
            continue;
        }
        let event = match event::read() {
            Ok(event) => event,
            Err(error) => {
                drop(guard);
                eprintln!("Failed to read Studio input: {error}");
                return 1;
            }
        };
        let action = match handle_event(&mut state, event) {
            Ok(action) => action,
            Err(error) => {
                state.status = error;
                StudioAction::None
            }
        };
        match action {
            StudioAction::None => {}
            StudioAction::Quit => return 130,
            StudioAction::RunAmonHen => {
                start_studio_run(&mut state);
            }
            StudioAction::CancelJob => {
                cancel_studio_job(&mut state);
            }
            StudioAction::SocialLogin => {
                start_studio_action_job(&mut state, StudioJobKind::SocialLogin);
            }
            StudioAction::AuthStatus => {
                start_studio_action_job(&mut state, StudioJobKind::AuthStatus);
            }
            StudioAction::CapabilitiesStatus => {
                start_studio_action_job(&mut state, StudioJobKind::CapabilitiesStatus);
            }
            StudioAction::LinearStatus => {
                start_studio_action_job(&mut state, StudioJobKind::LinearStatus);
            }
            StudioAction::LinearDeliver => {
                state.resolved.raw.deliver_linear = true;
                start_studio_action_job(&mut state, StudioJobKind::LinearDeliver);
            }
        }
    }
}

fn start_studio_run(state: &mut StudioState) {
    if state.run_job.is_some() {
        state.status = "A Studio job is already running".to_string();
        state.focus = Pane::Results;
        return;
    }

    let mut resolved = state.resolved.clone();
    resolved.prompt = state.prompt.clone();
    resolved.raw.verbose = false;
    let prompt = state.prompt.clone();
    let (tx, rx) = mpsc::channel::<StudioJobMessage>();
    let cancel = Arc::new(AtomicBool::new(false));
    let thread_cancel = Arc::clone(&cancel);
    let progress_tx = tx.clone();
    let progress: ProgressSink = Arc::new(move |event| {
        let _ = progress_tx.send(StudioJobMessage::Progress(event));
    });

    state.run_events.clear();
    state.provider_status.clear();
    state.provider_detail.clear();
    state.live_token_usage.clear();
    state.live_tool_counts.clear();
    state.live_sub_agents.clear();
    state.last_result = None;
    state.status = "Amon Hen running inside Studio".to_string();
    state.focus = Pane::Results;
    push_run_event(state, "[studio] run queued");
    for member in &state.resolved.members {
        state
            .provider_status
            .insert(member.clone(), "queued".to_string());
    }

    thread::spawn(move || {
        if thread_cancel.load(Ordering::Relaxed) {
            let _ = tx.send(StudioJobMessage::Cancelled(
                "Amon Hen run cancelled before start".to_string(),
            ));
            return;
        }
        let mut thread_resolved = resolved;
        thread_resolved.prompt = prompt;
        let prompt_context =
            match build_prompt_context_with_progress(&thread_resolved, Some(progress.clone())) {
                Ok(context) => context,
                Err(error) => {
                    let _ = tx.send(StudioJobMessage::Failed(format!(
                        "Prompt context failed: {error}"
                    )));
                    return;
                }
            };
        if thread_cancel.load(Ordering::Relaxed) {
            let _ = tx.send(StudioJobMessage::Cancelled(
                "Amon Hen run cancelled after prompt context".to_string(),
            ));
            return;
        }
        let result = run_amon_hen_with_progress_and_cancel(
            &thread_resolved,
            prompt_context.prompt,
            prompt_context.commands,
            Some(progress),
            Some(Arc::clone(&thread_cancel)),
        );
        if thread_cancel.load(Ordering::Relaxed) {
            let _ = tx.send(StudioJobMessage::Cancelled(
                "Amon Hen run cancelled".to_string(),
            ));
            return;
        }
        let _ = tx.send(StudioJobMessage::Finished(Box::new(result)));
    });

    state.run_job = Some(StudioRunJob {
        rx,
        started: Instant::now(),
        cancel,
        kind: StudioJobKind::AmonHen,
    });
}

fn start_studio_action_job(state: &mut StudioState, kind: StudioJobKind) {
    if state.run_job.is_some() {
        state.status = "A Studio job is already running".to_string();
        state.focus = Pane::Results;
        return;
    }

    let resolved = state.resolved.clone();
    let (tx, rx) = mpsc::channel::<StudioJobMessage>();
    let cancel = Arc::new(AtomicBool::new(false));
    let thread_cancel = Arc::clone(&cancel);

    state.status = kind.running_status().to_string();
    state.focus = match kind {
        StudioJobKind::AuthStatus | StudioJobKind::SocialLogin => Pane::Agents,
        StudioJobKind::CapabilitiesStatus => Pane::Capabilities,
        StudioJobKind::LinearStatus | StudioJobKind::LinearDeliver => Pane::Linear,
        StudioJobKind::AmonHen => Pane::Results,
    };
    push_run_event(state, format!("[studio] {} queued", kind.label()));

    thread::spawn(move || {
        let outcome = run_studio_action(kind, resolved, tx.clone(), Arc::clone(&thread_cancel));
        if thread_cancel.load(Ordering::Relaxed) {
            let _ = tx.send(StudioJobMessage::Cancelled(format!(
                "{} cancelled",
                kind.label()
            )));
            return;
        }
        let _ = tx.send(StudioJobMessage::ExternalFinished(outcome));
    });

    state.run_job = Some(StudioRunJob {
        rx,
        started: Instant::now(),
        cancel,
        kind,
    });
}

fn run_studio_action(
    kind: StudioJobKind,
    mut resolved: ResolvedArgs,
    tx: Sender<StudioJobMessage>,
    cancel: Arc<AtomicBool>,
) -> Result<StudioJobOutcome, String> {
    match kind {
        StudioJobKind::SocialLogin => {
            run_studio_social_login(&resolved, tx, cancel)?;
            Ok(StudioJobOutcome {
                status: "Social login completed".to_string(),
                focus: Pane::Agents,
                auth_result: Some(render_auth_statuses(&collect_auth_statuses(&resolved))),
                capability_result: None,
                linear_result: None,
            })
        }
        StudioJobKind::AuthStatus => Ok(StudioJobOutcome {
            status: "Auth status refreshed".to_string(),
            focus: Pane::Agents,
            auth_result: Some(render_auth_statuses(&collect_auth_statuses_with_cancel(
                &resolved,
                Some(Arc::clone(&cancel)),
            ))),
            capability_result: None,
            linear_result: None,
        }),
        StudioJobKind::CapabilitiesStatus => Ok(StudioJobOutcome {
            status: "Provider capabilities refreshed".to_string(),
            focus: Pane::Capabilities,
            auth_result: None,
            capability_result: Some(render_provider_capability_statuses(
                &collect_provider_capability_statuses_with_cancel(
                    &resolved,
                    Some(Arc::clone(&cancel)),
                ),
            )),
            linear_result: None,
        }),
        StudioJobKind::LinearStatus => {
            if cancel.load(Ordering::Relaxed) {
                return Err("Linear status cancelled".to_string());
            }
            let status = linear_delivery::get_linear_status(&resolved)?;
            Ok(StudioJobOutcome {
                status: "Linear status refreshed".to_string(),
                focus: Pane::Linear,
                auth_result: None,
                capability_result: None,
                linear_result: Some(linear_delivery::render_linear_status(&status)),
            })
        }
        StudioJobKind::LinearDeliver => {
            resolved.raw.deliver_linear = true;
            let progress_tx = tx.clone();
            let progress: ProgressSink = Arc::new(move |event| {
                let _ = progress_tx.send(StudioJobMessage::Progress(event));
            });
            let result = linear_delivery::run_linear_delivery_with_progress(
                &resolved,
                Some(progress),
                Some(Arc::clone(&cancel)),
            )?;
            Ok(StudioJobOutcome {
                status: if result.success {
                    "Linear delivery completed".to_string()
                } else {
                    "Linear delivery needs attention".to_string()
                },
                focus: Pane::Linear,
                auth_result: None,
                capability_result: None,
                linear_result: Some(linear_delivery::render_linear_delivery_result(&result)),
            })
        }
        StudioJobKind::AmonHen => unreachable!("Amon Hen uses start_studio_run"),
    }
}

fn cancel_studio_job(state: &mut StudioState) {
    let Some(job) = &state.run_job else {
        state.status = "No active Studio job to cancel".to_string();
        return;
    };
    let label = job.kind.label();
    if job.cancel.swap(true, Ordering::Relaxed) {
        state.status = format!("{label} cancellation already requested");
        return;
    }
    state.status = format!("{label} cancellation requested");
    state.focus = Pane::Results;
    push_run_event(
        state,
        format!("[studio] {label} cancellation requested; owned subprocesses will be stopped where possible"),
    );
}

fn run_studio_social_login(
    resolved: &ResolvedArgs,
    tx: Sender<StudioJobMessage>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    let providers = if resolved.raw.auth_login_providers.is_empty() {
        resolved.members.clone()
    } else {
        resolved.raw.auth_login_providers.clone()
    };

    for provider in providers {
        if cancel.load(Ordering::Relaxed) {
            return Err("Social login cancelled".to_string());
        }
        validate_engine_name(&provider, false, "--auth-login-providers")?;
        let (bin, args, instruction): (String, Vec<String>, &str) = match provider.as_str() {
            "codex" => {
                let mut args = vec!["login".to_string()];
                if resolved.raw.auth_device_code {
                    args.push("--device-auth".to_string());
                }
                (
                    resolve_binary("codex"),
                    args,
                    "Complete browser login when opened; device-code flows are shown in the Studio log.",
                )
            }
            "claude" => {
                let mut args = vec!["auth".to_string(), "login".to_string()];
                match resolved.raw.claude_login_mode.as_str() {
                    "console" => args.push("--console".to_string()),
                    "sso" => args.push("--sso".to_string()),
                    _ => args.push("--claudeai".to_string()),
                }
                if let Some(email) = &resolved.raw.claude_login_email {
                    push_arg(&mut args, "--email", email.clone());
                }
                (
                    resolve_binary("claude"),
                    args,
                    "Complete browser login when opened; CLI prompts are surfaced in the Studio log.",
                )
            }
            "gemini" => (
                resolve_binary("gemini"),
                vec![],
                "Use the Gemini CLI auth selector from the Studio log; browser URLs are opened when emitted.",
            ),
            _ => unreachable!(),
        };
        send_studio_log(
            &tx,
            format!(
                "[auth] launching {provider}: {}",
                format_command(&bin, &args)
            ),
        );
        send_studio_log(&tx, format!("[auth] {provider}: {instruction}"));
        let result = run_studio_auth_command(StudioAuthCommand {
            command: &bin,
            args: &args,
            cwd: &resolved.cwd,
            timeout_ms: resolved.raw.auth_timeout * 1000,
            open_browser: !resolved.raw.no_auth_open_browser,
            provider: &provider,
            tx: tx.clone(),
            cancel: Arc::clone(&cancel),
        })?;
        if result.code.unwrap_or(1) != 0 {
            return Err(format!(
                "{provider} social login failed: {}",
                compact_failure(&result)
            ));
        }
        let status = provider_auth_status(&provider, resolved);
        send_studio_log(
            &tx,
            format!("[auth] {provider}: {} ({})", status.status, status.detail),
        );
    }
    Ok(())
}

struct StudioAuthCommand<'a> {
    command: &'a str,
    args: &'a [String],
    cwd: &'a Path,
    timeout_ms: u64,
    open_browser: bool,
    provider: &'a str,
    tx: Sender<StudioJobMessage>,
    cancel: Arc<AtomicBool>,
}

fn run_studio_auth_command(context: StudioAuthCommand<'_>) -> Result<CommandResult, String> {
    let started = Instant::now();
    let mut process = Command::new(context.command);
    process
        .args(context.args)
        .current_dir(context.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut process);
    let mut child = process
        .spawn()
        .map_err(|error| format!("{} social login failed to start: {error}", context.provider))?;

    let seen_urls = Arc::new(Mutex::new(HashSet::new()));
    let stdout = child.stdout.take().map(|pipe| {
        read_studio_auth_pipe(
            pipe,
            context.provider.to_string(),
            context.open_browser,
            Arc::clone(&seen_urls),
            context.tx.clone(),
        )
    });
    let stderr = child.stderr.take().map(|pipe| {
        read_studio_auth_pipe(
            pipe,
            context.provider.to_string(),
            context.open_browser,
            Arc::clone(&seen_urls),
            context.tx.clone(),
        )
    });
    let timeout = Duration::from_millis(context.timeout_ms);
    let mut timed_out = false;
    let code;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                code = status.code();
                break;
            }
            Ok(None) => {
                if context.cancel.load(Ordering::Relaxed) {
                    terminate_child_tree(&mut child);
                    let _ = child.wait();
                    return Err(format!("{} social login cancelled", context.provider));
                }
                if context.timeout_ms > 0 && started.elapsed() >= timeout {
                    timed_out = true;
                    terminate_child_tree(&mut child);
                    let status = child.wait().ok();
                    code = status.and_then(|status| status.code());
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                return Ok(CommandResult {
                    command: context.command.to_string(),
                    args: context.args.to_vec(),
                    code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out,
                    cancelled: false,
                    error: Some(error.to_string()),
                    timeout_ms: context.timeout_ms,
                    duration_ms: started.elapsed().as_millis(),
                });
            }
        }
    }

    Ok(CommandResult {
        command: context.command.to_string(),
        args: context.args.to_vec(),
        code,
        stdout: stdout
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default(),
        stderr: stderr
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default(),
        timed_out,
        cancelled: false,
        error: None,
        timeout_ms: context.timeout_ms,
        duration_ms: started.elapsed().as_millis(),
    })
}

fn read_studio_auth_pipe<R>(
    mut pipe: R,
    provider: String,
    open_browser: bool,
    seen_urls: Arc<Mutex<HashSet<String>>>,
    tx: Sender<StudioJobMessage>,
) -> thread::JoinHandle<String>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut text = String::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(_) => break,
            };
            let chunk = String::from_utf8_lossy(&buffer[..read]).to_string();
            text.push_str(&chunk);
            for line in chunk.lines().map(str::trim).filter(|line| !line.is_empty()) {
                send_studio_log(
                    &tx,
                    format!("[auth] {provider}: {}", studio_clip(line, 220)),
                );
            }
            if open_browser {
                for url in extract_auth_urls(&chunk) {
                    let mut seen = seen_urls.lock().ok();
                    if seen.as_mut().is_some_and(|seen| !seen.insert(url.clone())) {
                        continue;
                    }
                    send_studio_log(&tx, format!("[auth] {provider}: opening {url}"));
                    if let Err(error) = open_browser_url(&url) {
                        send_studio_log(
                            &tx,
                            format!("[auth] {provider}: failed to open {url}: {error}"),
                        );
                    }
                }
            }
        }
        text
    })
}

fn send_studio_log(tx: &Sender<StudioJobMessage>, line: impl Into<String>) {
    let _ = tx.send(StudioJobMessage::Log(line.into()));
}

fn drain_studio_job(state: &mut StudioState) {
    let mut messages = Vec::new();
    let mut elapsed = None;
    let mut kind = None;
    let mut cancel_requested = false;
    if let Some(job) = &state.run_job {
        elapsed = Some(job.started.elapsed());
        kind = Some(job.kind);
        cancel_requested = job.cancel.load(Ordering::Relaxed);
        while let Ok(message) = job.rx.try_recv() {
            messages.push(message);
        }
    }

    let mut finished = false;
    for message in messages {
        match message {
            StudioJobMessage::Progress(event) => apply_progress_event(state, event),
            StudioJobMessage::Log(line) => push_run_event(state, line),
            StudioJobMessage::Finished(result) => {
                let result = *result;
                state.status = if is_success(&result) {
                    "Amon Hen run completed".to_string()
                } else {
                    "Amon Hen run needs attention".to_string()
                };
                for member in &result.members {
                    state
                        .provider_status
                        .insert(member.name.clone(), member.status.clone());
                    let detail = if member.detail.trim().is_empty() {
                        format!(
                            "{} in {:.1}s",
                            member.command,
                            member.duration_ms as f64 / 1000.0
                        )
                    } else {
                        member.detail.clone()
                    };
                    state.provider_detail.insert(member.name.clone(), detail);
                }
                state.last_result = Some(result);
                state.focus = Pane::Results;
                push_run_event(state, "[studio] run finished");
                finished = true;
            }
            StudioJobMessage::ExternalFinished(outcome) => {
                match outcome {
                    Ok(outcome) => {
                        if let Some(result) = outcome.auth_result {
                            state.last_auth_result = Some(result);
                        }
                        if let Some(result) = outcome.capability_result {
                            state.last_capability_result = Some(result);
                        }
                        if let Some(result) = outcome.linear_result {
                            state.last_linear_result = Some(result);
                        }
                        state.status = outcome.status;
                        state.focus = outcome.focus;
                    }
                    Err(error) => {
                        state.status = error.clone();
                        push_run_event(state, format!("[studio] {error}"));
                    }
                }
                finished = true;
            }
            StudioJobMessage::Cancelled(message) => {
                state.status = message.clone();
                push_run_event(state, format!("[studio] {message}"));
                finished = true;
            }
            StudioJobMessage::Failed(error) => {
                state.status = error.clone();
                push_run_event(state, format!("[studio] {error}"));
                finished = true;
            }
        }
    }

    if finished {
        state.run_job = None;
    } else if let Some(elapsed) = elapsed {
        let label = kind.map(StudioJobKind::label).unwrap_or("Studio job");
        state.status = if cancel_requested {
            format!("{label} cancelling ({:.1}s)", elapsed.as_secs_f64())
        } else {
            format!("{label} running ({:.1}s)", elapsed.as_secs_f64())
        };
    }
}

fn apply_progress_event(state: &mut StudioState, event: ProgressEvent) {
    push_run_event(state, sanitize_status_detail(&event.message));
    if let Some(provider) = event.provider.clone() {
        if let Some(usage) = event.token_usage.clone() {
            state.live_token_usage.insert(provider.clone(), usage);
        }
        if !event.tool_calls.is_empty() {
            let count = state.live_tool_counts.entry(provider.clone()).or_default();
            *count = count.saturating_add(event.tool_calls.len());
        }
        if event.is_sub_agent {
            if let Some(role) = event.role.clone() {
                state
                    .live_sub_agents
                    .entry(provider.clone())
                    .or_default()
                    .insert(role);
            }
        }
        let status = event.status.unwrap_or_else(|| match event.stage {
            ProgressStage::Done => "done".to_string(),
            _ => "running".to_string(),
        });
        state.provider_status.insert(provider.clone(), status);
        let role = event.role.unwrap_or_else(|| "agent".to_string());
        state.provider_detail.insert(
            provider,
            format!("{} | {}", role, sanitize_status_detail(&event.message)),
        );
    }
}

fn push_run_event(state: &mut StudioState, line: impl Into<String>) {
    const MAX_RUN_EVENTS: usize = 500;
    state.run_events.push(studio_clip(&line.into(), 240));
    if state.run_events.len() > MAX_RUN_EVENTS {
        let overflow = state.run_events.len() - MAX_RUN_EVENTS;
        state.run_events.drain(0..overflow);
    }
    state.result_index = result_len(state).saturating_sub(1);
}

fn handle_event(state: &mut StudioState, event: Event) -> Result<StudioAction, String> {
    let Event::Key(key) = event else {
        return Ok(StudioAction::None);
    };
    if let Some(mode) = state.input_mode.clone() {
        return handle_input_event(state, key, mode);
    }
    if is_ctrl_c_key(key) {
        let now = Instant::now();
        if state.exit_armed_until.is_some_and(|until| now <= until) {
            return Ok(StudioAction::Quit);
        }
        state.exit_armed_until = Some(now + Duration::from_secs(5));
        state.status = "Press Ctrl+C again within 5s to quit".to_string();
        return Ok(StudioAction::None);
    }
    match key.code {
        KeyCode::Char('q') => {
            state.status = "Press Ctrl+C twice to quit, or Enter on Quit from the menu".to_string();
        }
        KeyCode::Char('?') => state.show_help = !state.show_help,
        KeyCode::Char('r') => return Ok(StudioAction::RunAmonHen),
        KeyCode::Char('c') => return Ok(StudioAction::CancelJob),
        KeyCode::Char('e') => return start_input(state, InputMode::Prompt, state.prompt.clone()),
        KeyCode::Tab => cycle_focus(state, 1),
        KeyCode::BackTab => cycle_focus(state, -1),
        KeyCode::Char('[') => move_focused_pane(state, -1),
        KeyCode::Char(']') => move_focused_pane(state, 1),
        KeyCode::Up => move_selection(state, -1),
        KeyCode::Down => move_selection(state, 1),
        KeyCode::Left => adjust_selection(state, -1)?,
        KeyCode::Right => adjust_selection(state, 1)?,
        KeyCode::Enter => return activate_selection(state),
        KeyCode::Esc => state.show_help = false,
        _ => {}
    }
    Ok(StudioAction::None)
}

fn is_ctrl_c_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('\u{3}'))
        || (key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C')))
}

fn handle_input_event(
    state: &mut StudioState,
    key: KeyEvent,
    mode: InputMode,
) -> Result<StudioAction, String> {
    match key.code {
        KeyCode::Esc => {
            state.input_mode = None;
            state.input_buffer.clear();
            state.status = "Input cancelled".to_string();
        }
        KeyCode::Enter => {
            let value = state.input_buffer.trim().to_string();
            apply_input(state, mode, value);
            state.input_mode = None;
            state.input_buffer.clear();
        }
        KeyCode::Backspace => {
            state.input_buffer.pop();
        }
        KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.input_buffer.push(ch);
        }
        _ => {}
    }
    Ok(StudioAction::None)
}

fn apply_input(state: &mut StudioState, mode: InputMode, value: String) {
    match mode {
        InputMode::Prompt => {
            state.prompt = value;
            state.status = "Prompt updated".to_string();
        }
        InputMode::File => {
            if !value.is_empty() {
                state.resolved.raw.files.push(PathBuf::from(value));
                state.status = "Tagged file added".to_string();
            }
        }
        InputMode::Command => {
            if !value.is_empty() {
                state.resolved.raw.commands.push(value);
                state.status = "Prompt command added".to_string();
            }
        }
        InputMode::LinearIssue => set_csv(
            &mut state.resolved.raw.linear_issue,
            value,
            "Linear issue",
            &mut state.status,
        ),
        InputMode::LinearQuery => {
            state.resolved.raw.linear_query = empty_to_none(value);
            state.status = "Linear query updated".to_string();
        }
        InputMode::LinearProject => set_csv(
            &mut state.resolved.raw.linear_project,
            value,
            "Linear project",
            &mut state.status,
        ),
        InputMode::LinearEpic => set_csv(
            &mut state.resolved.raw.linear_epic,
            value,
            "Linear epic",
            &mut state.status,
        ),
        InputMode::LinearTeam => {
            state.resolved.raw.linear_team = empty_to_none(value);
            state.status = "Linear team updated".to_string();
        }
        InputMode::LinearState => {
            state.resolved.raw.linear_state = empty_to_none(value);
            state.status = "Linear state updated".to_string();
        }
        InputMode::LinearMedia => set_csv(
            &mut state.resolved.raw.linear_attach_media,
            value,
            "Linear media",
            &mut state.status,
        ),
        InputMode::CodexModel => {
            state.resolved.raw.codex_model = empty_to_none(value);
            state.status = "Codex model updated".to_string();
        }
        InputMode::ClaudeModel => {
            state.resolved.raw.claude_model = empty_to_none(value);
            state.status = "Claude model updated".to_string();
        }
        InputMode::GeminiModel => {
            state.resolved.raw.gemini_model = empty_to_none(value);
            state.status = "Gemini model updated".to_string();
        }
        InputMode::CodexConfig => set_csv(
            &mut state.resolved.raw.codex_config,
            value,
            "Codex config",
            &mut state.status,
        ),
        InputMode::CodexProfile => {
            state.resolved.raw.codex_mcp_profile = empty_to_none(value);
            state.status = "Codex MCP profile updated".to_string();
        }
        InputMode::ClaudeMcpConfig => set_csv(
            &mut state.resolved.raw.claude_mcp_config,
            value,
            "Claude MCP config",
            &mut state.status,
        ),
        InputMode::ClaudeAllowedTools => set_csv(
            &mut state.resolved.raw.claude_allowed_tools,
            value,
            "Claude allowed tools",
            &mut state.status,
        ),
        InputMode::ClaudeDisallowedTools => set_csv(
            &mut state.resolved.raw.claude_disallowed_tools,
            value,
            "Claude disallowed tools",
            &mut state.status,
        ),
        InputMode::ClaudeTools => set_csv(
            &mut state.resolved.raw.claude_tools,
            value,
            "Claude tools",
            &mut state.status,
        ),
        InputMode::ClaudeAgent => {
            state.resolved.raw.claude_agent = empty_to_none(value);
            state.status = "Claude agent updated".to_string();
        }
        InputMode::ClaudeAgentsJson => {
            state.resolved.raw.claude_agents_json = empty_to_none(value);
            state.status = "Claude agents JSON updated".to_string();
        }
        InputMode::ClaudePluginDir => set_csv(
            &mut state.resolved.raw.claude_plugin_dir,
            value,
            "Claude plugin dirs",
            &mut state.status,
        ),
        InputMode::GeminiSettings => {
            state.resolved.raw.gemini_settings = empty_to_none(value);
            state.status = "Gemini settings updated".to_string();
        }
        InputMode::GeminiToolsProfile => set_csv(
            &mut state.resolved.raw.gemini_tools_profile,
            value,
            "Gemini tools profile",
            &mut state.status,
        ),
        InputMode::GeminiAllowedMcp => set_csv(
            &mut state.resolved.raw.gemini_allowed_mcp_servers,
            value,
            "Gemini allowed MCP servers",
            &mut state.status,
        ),
        InputMode::GeminiPolicy => set_csv(
            &mut state.resolved.raw.gemini_policy,
            value,
            "Gemini policy",
            &mut state.status,
        ),
        InputMode::GeminiAdminPolicy => set_csv(
            &mut state.resolved.raw.gemini_admin_policy,
            value,
            "Gemini admin policy",
            &mut state.status,
        ),
        InputMode::SaveProfile => {
            let name = if value.trim().is_empty() {
                state.profile_name.clone()
            } else {
                value
            };
            match save_studio_profile(state, &name) {
                Ok(()) => state.status = format!("Profile `{name}` saved"),
                Err(error) => state.status = format!("Profile save failed: {error}"),
            }
        }
        InputMode::LoadProfile => {
            let name = if value.trim().is_empty() {
                state.profile_name.clone()
            } else {
                value
            };
            match load_and_apply_studio_profile(state, &name) {
                Ok(()) => state.status = format!("Profile `{name}` loaded"),
                Err(error) => state.status = format!("Profile load failed: {error}"),
            }
        }
    }
}

fn set_csv(target: &mut Vec<String>, value: String, label: &str, status: &mut String) {
    *target = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect();
    *status = format!("{label} updated");
}

fn empty_to_none(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn non_empty_profile_value(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn studio_profile_path(cwd: &Path) -> PathBuf {
    if let Some(path) = std::env::var_os("AMON_HEN_STUDIO_PROFILES") {
        return PathBuf::from(path);
    }
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config_home)
            .join("amon-hen")
            .join("studio-profiles.json");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("amon-hen")
            .join("studio-profiles.json");
    }
    cwd.join(".amon-hen-studio-profiles.json")
}

fn studio_profile_names(path: &Path) -> Result<Vec<String>, String> {
    let mut names = read_studio_profiles(path)?
        .profiles
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

fn read_studio_profiles(path: &Path) -> Result<StudioProfilesFile, String> {
    if !path.exists() {
        return Ok(StudioProfilesFile::default());
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))
}

fn write_studio_profiles(path: &Path, profiles: &StudioProfilesFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create {}: {error}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(profiles)
        .map_err(|error| format!("Failed to serialize Studio profiles: {error}"))?;
    fs::write(path, text).map_err(|error| format!("Failed to write {}: {error}", path.display()))
}

fn save_studio_profile(state: &mut StudioState, name: &str) -> Result<(), String> {
    let name = profile_name(name)?;
    let mut profiles = read_studio_profiles(&state.profile_path)?;
    profiles
        .profiles
        .insert(name.clone(), profile_from_state(state));
    write_studio_profiles(&state.profile_path, &profiles)?;
    state.profile_name = name;
    state.profile_names = studio_profile_names(&state.profile_path)?;
    Ok(())
}

fn load_and_apply_studio_profile(state: &mut StudioState, name: &str) -> Result<(), String> {
    let name = profile_name(name)?;
    let profiles = read_studio_profiles(&state.profile_path)?;
    let profile = profiles
        .profiles
        .get(&name)
        .cloned()
        .ok_or_else(|| format!("Profile `{name}` was not found"))?;
    apply_studio_profile(state, &profile);
    state.profile_name = name;
    state.profile_names = studio_profile_names(&state.profile_path)?;
    Ok(())
}

fn profile_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        Err("Profile name cannot be empty".to_string())
    } else {
        Ok(name.to_string())
    }
}

fn profile_from_state(state: &StudioState) -> StudioProfile {
    let raw = &state.resolved.raw;
    StudioProfile {
        prompt: state.prompt.clone(),
        members: state.resolved.members.clone(),
        handoff: raw.handoff,
        lead: raw.lead.clone(),
        planner: raw.planner.clone(),
        summarizer: raw.summarizer.clone(),
        iterations: raw.iterations,
        team_work: raw.team_work,
        codex_sub_agents: raw.codex_sub_agents,
        claude_sub_agents: raw.claude_sub_agents,
        gemini_sub_agents: raw.gemini_sub_agents,
        codex_model: raw.codex_model.clone(),
        claude_model: raw.claude_model.clone(),
        gemini_model: raw.gemini_model.clone(),
        codex_effort: raw.codex_effort.clone(),
        claude_effort: raw.claude_effort.clone(),
        gemini_effort: raw.gemini_effort.clone(),
        codex_auth: raw.codex_auth.clone(),
        claude_auth: raw.claude_auth.clone(),
        gemini_auth: raw.gemini_auth.clone(),
        codex_sandbox: raw.codex_sandbox.clone(),
        claude_permission_mode: raw.claude_permission_mode.clone(),
        codex_capabilities: raw.codex_capabilities.clone(),
        codex_config: raw.codex_config.clone(),
        codex_mcp_profile: raw.codex_mcp_profile.clone(),
        claude_capabilities: raw.claude_capabilities.clone(),
        claude_mcp_config: raw.claude_mcp_config.clone(),
        claude_allowed_tools: raw.claude_allowed_tools.clone(),
        claude_disallowed_tools: raw.claude_disallowed_tools.clone(),
        claude_tools: raw.claude_tools.clone(),
        claude_agent: raw.claude_agent.clone(),
        claude_agents_json: raw.claude_agents_json.clone(),
        claude_plugin_dir: raw.claude_plugin_dir.clone(),
        claude_strict_mcp_config: raw.claude_strict_mcp_config,
        claude_disable_slash_commands: raw.claude_disable_slash_commands,
        gemini_capabilities: raw.gemini_capabilities.clone(),
        gemini_settings: raw.gemini_settings.clone(),
        gemini_tools_profile: raw.gemini_tools_profile.clone(),
        gemini_allowed_mcp_servers: raw.gemini_allowed_mcp_servers.clone(),
        gemini_policy: raw.gemini_policy.clone(),
        gemini_admin_policy: raw.gemini_admin_policy.clone(),
        deliver_linear: raw.deliver_linear,
        linear_watch: raw.linear_watch,
        linear_auth: raw.linear_auth.clone(),
        linear_issue: raw.linear_issue.clone(),
        linear_query: raw.linear_query.clone(),
        linear_project: raw.linear_project.clone(),
        linear_epic: raw.linear_epic.clone(),
        linear_team: raw.linear_team.clone(),
        linear_state: raw.linear_state.clone(),
        linear_completion_gate: raw.linear_completion_gate.clone(),
        linear_workspace_strategy: raw.linear_workspace_strategy.clone(),
        linear_poll_interval: raw.linear_poll_interval,
        linear_max_attempts: raw.linear_max_attempts,
        no_linear_comments: raw.no_linear_comments,
        linear_update_review_state: raw.linear_update_review_state,
        linear_attach_media: raw.linear_attach_media.clone(),
        linear_until_complete: raw.linear_until_complete,
        linear_assignee: raw.linear_assignee.clone(),
        linear_limit: raw.linear_limit,
        linear_endpoint: raw.linear_endpoint.clone(),
        linear_api_key_env: raw.linear_api_key_env.clone(),
        linear_oauth_token_env: raw.linear_oauth_token_env.clone(),
        linear_review_state: raw.linear_review_state.clone(),
        linear_ci_timeout: raw.linear_ci_timeout,
        linear_ci_poll_interval: raw.linear_ci_poll_interval,
        linear_max_polls: raw.linear_max_polls,
        linear_max_concurrency: raw.linear_max_concurrency,
        linear_retry_base: raw.linear_retry_base,
        linear_state_file: raw.linear_state_file.clone(),
        linear_workspace_root: raw.linear_workspace_root.clone(),
        linear_observability_dir: raw.linear_observability_dir.clone(),
        linear_workflow_file: raw.linear_workflow_file.clone(),
        linear_attachment_title: raw.linear_attachment_title.clone(),
        delivery_phases: raw.delivery_phases.clone(),
    }
}

fn apply_studio_profile(state: &mut StudioState, profile: &StudioProfile) {
    let raw = &mut state.resolved.raw;
    state.prompt = profile.prompt.clone();
    state.resolved.prompt = profile.prompt.clone();
    if !profile.members.is_empty() {
        state.resolved.members = profile.members.clone();
        raw.members = profile.members.clone();
    }
    raw.handoff = profile.handoff;
    raw.lead = profile.lead.clone();
    raw.planner = profile.planner.clone();
    raw.summarizer = profile.summarizer.clone();
    raw.iterations = profile.iterations.max(1);
    raw.team_work = profile.team_work;
    raw.codex_sub_agents = profile.codex_sub_agents;
    raw.claude_sub_agents = profile.claude_sub_agents;
    raw.gemini_sub_agents = profile.gemini_sub_agents;
    raw.codex_model = profile.codex_model.clone();
    raw.claude_model = profile.claude_model.clone();
    raw.gemini_model = profile.gemini_model.clone();
    raw.codex_effort = profile.codex_effort.clone();
    raw.claude_effort = profile.claude_effort.clone();
    raw.gemini_effort = profile.gemini_effort.clone();
    raw.codex_auth = profile.codex_auth.clone();
    raw.claude_auth = profile.claude_auth.clone();
    raw.gemini_auth = profile.gemini_auth.clone();
    raw.codex_sandbox = profile.codex_sandbox.clone();
    raw.claude_permission_mode = profile.claude_permission_mode.clone();
    raw.codex_capabilities = profile.codex_capabilities.clone();
    raw.codex_config = profile.codex_config.clone();
    raw.codex_mcp_profile = profile.codex_mcp_profile.clone();
    raw.claude_capabilities = profile.claude_capabilities.clone();
    raw.claude_mcp_config = profile.claude_mcp_config.clone();
    raw.claude_allowed_tools = profile.claude_allowed_tools.clone();
    raw.claude_disallowed_tools = profile.claude_disallowed_tools.clone();
    raw.claude_tools = profile.claude_tools.clone();
    raw.claude_agent = profile.claude_agent.clone();
    raw.claude_agents_json = profile.claude_agents_json.clone();
    raw.claude_plugin_dir = profile.claude_plugin_dir.clone();
    raw.claude_strict_mcp_config = profile.claude_strict_mcp_config;
    raw.claude_disable_slash_commands = profile.claude_disable_slash_commands;
    raw.gemini_capabilities = profile.gemini_capabilities.clone();
    raw.gemini_settings = profile.gemini_settings.clone();
    raw.gemini_tools_profile = profile.gemini_tools_profile.clone();
    raw.gemini_allowed_mcp_servers = profile.gemini_allowed_mcp_servers.clone();
    raw.gemini_policy = profile.gemini_policy.clone();
    raw.gemini_admin_policy = profile.gemini_admin_policy.clone();
    raw.deliver_linear = profile.deliver_linear;
    raw.linear_watch = profile.linear_watch;
    raw.linear_auth = non_empty_profile_value(&profile.linear_auth, &raw.linear_auth);
    raw.linear_issue = profile.linear_issue.clone();
    raw.linear_query = profile.linear_query.clone();
    raw.linear_project = profile.linear_project.clone();
    raw.linear_epic = profile.linear_epic.clone();
    raw.linear_team = profile.linear_team.clone();
    raw.linear_state = profile.linear_state.clone();
    raw.linear_completion_gate =
        non_empty_profile_value(&profile.linear_completion_gate, &raw.linear_completion_gate);
    raw.linear_workspace_strategy = non_empty_profile_value(
        &profile.linear_workspace_strategy,
        &raw.linear_workspace_strategy,
    );
    raw.linear_poll_interval = profile.linear_poll_interval.max(1);
    raw.linear_max_attempts = profile.linear_max_attempts.max(1);
    raw.no_linear_comments = profile.no_linear_comments;
    raw.linear_update_review_state = profile.linear_update_review_state;
    raw.linear_attach_media = profile.linear_attach_media.clone();
    raw.linear_until_complete = profile.linear_until_complete;
    raw.linear_assignee = profile.linear_assignee.clone();
    raw.linear_limit = profile.linear_limit.max(1);
    raw.linear_endpoint = profile.linear_endpoint.clone();
    raw.linear_api_key_env =
        non_empty_profile_value(&profile.linear_api_key_env, &raw.linear_api_key_env);
    raw.linear_oauth_token_env =
        non_empty_profile_value(&profile.linear_oauth_token_env, &raw.linear_oauth_token_env);
    raw.linear_review_state = profile.linear_review_state.clone();
    raw.linear_ci_timeout = profile.linear_ci_timeout;
    raw.linear_ci_poll_interval = profile.linear_ci_poll_interval;
    raw.linear_max_polls = profile.linear_max_polls;
    raw.linear_max_concurrency = profile.linear_max_concurrency.max(1);
    raw.linear_retry_base = profile.linear_retry_base;
    raw.linear_state_file = profile.linear_state_file.clone();
    raw.linear_workspace_root = profile.linear_workspace_root.clone();
    raw.linear_observability_dir = profile.linear_observability_dir.clone();
    raw.linear_workflow_file = profile.linear_workflow_file.clone();
    raw.linear_attachment_title = profile.linear_attachment_title.clone();
    raw.delivery_phases = profile.delivery_phases.clone();
}

fn cycle_focus(state: &mut StudioState, delta: isize) {
    let current = state
        .pane_order
        .iter()
        .position(|pane| *pane == state.focus)
        .unwrap_or(0);
    let next = wrap_index(current, state.pane_order.len(), delta);
    state.focus = state.pane_order[next];
}

fn move_focused_pane(state: &mut StudioState, delta: isize) {
    let Some(index) = state
        .pane_order
        .iter()
        .position(|pane| *pane == state.focus)
    else {
        return;
    };
    let next = wrap_index(index, state.pane_order.len(), delta);
    state.pane_order.swap(index, next);
    state.status = "Pane order changed".to_string();
}

fn move_selection(state: &mut StudioState, delta: isize) {
    match state.focus {
        Pane::Menu => state.menu_index = wrap_index(state.menu_index, MENU.len(), delta),
        Pane::Settings => {
            state.setting_index = wrap_index(state.setting_index, settings_len(), delta)
        }
        Pane::Capabilities => {
            state.capability_index = wrap_index(state.capability_index, capabilities_len(), delta)
        }
        Pane::Linear => state.linear_index = wrap_index(state.linear_index, linear_len(), delta),
        Pane::Results => {
            state.result_index = wrap_index(state.result_index, result_len(state), delta)
        }
        Pane::Agents => {}
    }
}

fn adjust_selection(state: &mut StudioState, delta: isize) -> Result<(), String> {
    match state.focus {
        Pane::Settings => adjust_setting(state, delta),
        Pane::Capabilities => adjust_capability(state, delta),
        Pane::Linear => adjust_linear(state, delta),
        Pane::Menu | Pane::Agents | Pane::Results => Ok(()),
    }
}

fn activate_selection(state: &mut StudioState) -> Result<StudioAction, String> {
    match state.focus {
        Pane::Menu => activate_menu(state),
        Pane::Settings => activate_setting(state),
        Pane::Capabilities => activate_capability(state),
        Pane::Linear => activate_linear(state),
        Pane::Agents | Pane::Results => Ok(StudioAction::None),
    }
}

fn activate_setting(state: &mut StudioState) -> Result<StudioAction, String> {
    match state.setting_index {
        4 => start_input(
            state,
            InputMode::CodexModel,
            state.resolved.raw.codex_model.clone().unwrap_or_default(),
        ),
        5 => start_input(
            state,
            InputMode::ClaudeModel,
            state.resolved.raw.claude_model.clone().unwrap_or_default(),
        ),
        6 => start_input(
            state,
            InputMode::GeminiModel,
            state.resolved.raw.gemini_model.clone().unwrap_or_default(),
        ),
        _ => {
            adjust_setting(state, 1)?;
            Ok(StudioAction::None)
        }
    }
}

fn activate_menu(state: &mut StudioState) -> Result<StudioAction, String> {
    match MENU[state.menu_index] {
        "Run / re-run" => Ok(StudioAction::RunAmonHen),
        "Cancel job" => Ok(StudioAction::CancelJob),
        "Edit prompt" => start_input(state, InputMode::Prompt, state.prompt.clone()),
        "Social login" => Ok(StudioAction::SocialLogin),
        "Auth status" => Ok(StudioAction::AuthStatus),
        "Linear status" => Ok(StudioAction::LinearStatus),
        "Deliver Linear" => Ok(StudioAction::LinearDeliver),
        "Save profile" => start_input(state, InputMode::SaveProfile, state.profile_name.clone()),
        "Load profile" => start_input(state, InputMode::LoadProfile, state.profile_name.clone()),
        "Tag local file" => start_input(state, InputMode::File, String::new()),
        "Run command" => start_input(state, InputMode::Command, String::new()),
        "Settings" => {
            state.focus = Pane::Settings;
            Ok(StudioAction::None)
        }
        "Agents" => {
            state.focus = Pane::Agents;
            Ok(StudioAction::None)
        }
        "Capabilities" => {
            state.focus = Pane::Capabilities;
            Ok(StudioAction::None)
        }
        "Refresh capabilities" => Ok(StudioAction::CapabilitiesStatus),
        "Linear" => {
            state.focus = Pane::Linear;
            Ok(StudioAction::None)
        }
        "Help" => {
            state.show_help = !state.show_help;
            Ok(StudioAction::None)
        }
        "Quit" => Ok(StudioAction::Quit),
        _ => Ok(StudioAction::None),
    }
}

fn activate_capability(state: &mut StudioState) -> Result<StudioAction, String> {
    match state.capability_index {
        1 => start_input(
            state,
            InputMode::CodexConfig,
            state.resolved.raw.codex_config.join(","),
        ),
        2 => start_input(
            state,
            InputMode::CodexProfile,
            state
                .resolved
                .raw
                .codex_mcp_profile
                .clone()
                .unwrap_or_default(),
        ),
        4 => start_input(
            state,
            InputMode::ClaudeMcpConfig,
            state.resolved.raw.claude_mcp_config.join(","),
        ),
        5 => start_input(
            state,
            InputMode::ClaudeAllowedTools,
            state.resolved.raw.claude_allowed_tools.join(","),
        ),
        6 => start_input(
            state,
            InputMode::ClaudeDisallowedTools,
            state.resolved.raw.claude_disallowed_tools.join(","),
        ),
        7 => start_input(
            state,
            InputMode::ClaudeTools,
            state.resolved.raw.claude_tools.join(","),
        ),
        8 => start_input(
            state,
            InputMode::ClaudeAgent,
            state.resolved.raw.claude_agent.clone().unwrap_or_default(),
        ),
        9 => start_input(
            state,
            InputMode::ClaudeAgentsJson,
            state
                .resolved
                .raw
                .claude_agents_json
                .clone()
                .unwrap_or_default(),
        ),
        10 => start_input(
            state,
            InputMode::ClaudePluginDir,
            state.resolved.raw.claude_plugin_dir.join(","),
        ),
        14 => start_input(
            state,
            InputMode::GeminiSettings,
            state
                .resolved
                .raw
                .gemini_settings
                .clone()
                .unwrap_or_default(),
        ),
        15 => start_input(
            state,
            InputMode::GeminiToolsProfile,
            state.resolved.raw.gemini_tools_profile.join(","),
        ),
        16 => start_input(
            state,
            InputMode::GeminiAllowedMcp,
            state.resolved.raw.gemini_allowed_mcp_servers.join(","),
        ),
        17 => start_input(
            state,
            InputMode::GeminiPolicy,
            state.resolved.raw.gemini_policy.join(","),
        ),
        18 => start_input(
            state,
            InputMode::GeminiAdminPolicy,
            state.resolved.raw.gemini_admin_policy.join(","),
        ),
        _ => {
            adjust_capability(state, 1)?;
            Ok(StudioAction::None)
        }
    }
}

fn activate_linear(state: &mut StudioState) -> Result<StudioAction, String> {
    match state.linear_index {
        2 => start_input(
            state,
            InputMode::LinearIssue,
            state.resolved.raw.linear_issue.join(","),
        ),
        3 => start_input(
            state,
            InputMode::LinearQuery,
            state.resolved.raw.linear_query.clone().unwrap_or_default(),
        ),
        4 => start_input(
            state,
            InputMode::LinearProject,
            state.resolved.raw.linear_project.join(","),
        ),
        5 => start_input(
            state,
            InputMode::LinearEpic,
            state.resolved.raw.linear_epic.join(","),
        ),
        6 => start_input(
            state,
            InputMode::LinearTeam,
            state.resolved.raw.linear_team.clone().unwrap_or_default(),
        ),
        7 => start_input(
            state,
            InputMode::LinearState,
            state.resolved.raw.linear_state.clone().unwrap_or_default(),
        ),
        14 => start_input(
            state,
            InputMode::LinearMedia,
            state.resolved.raw.linear_attach_media.join(","),
        ),
        15 => Ok(StudioAction::LinearStatus),
        16 => Ok(StudioAction::LinearDeliver),
        _ => {
            adjust_linear(state, 1)?;
            Ok(StudioAction::None)
        }
    }
}

fn start_input(
    state: &mut StudioState,
    mode: InputMode,
    initial: String,
) -> Result<StudioAction, String> {
    state.input_mode = Some(mode);
    state.input_buffer = initial;
    state.status = "Editing value; Enter saves, Esc cancels".to_string();
    Ok(StudioAction::None)
}

fn adjust_setting(state: &mut StudioState, delta: isize) -> Result<(), String> {
    match state.setting_index {
        0 => state.resolved.raw.handoff = !state.resolved.raw.handoff,
        1 => {
            state.resolved.raw.lead = cycle_optional_engine(
                state.resolved.raw.lead.as_deref(),
                &state.resolved.members,
                delta,
            )
        }
        2 => {
            state.resolved.raw.planner = cycle_optional_engine(
                state.resolved.raw.planner.as_deref(),
                &state.resolved.members,
                delta,
            )
        }
        3 => {
            state.resolved.raw.summarizer = cycle_summarizer(&state.resolved.raw.summarizer, delta)
        }
        4..=6 => {}
        7 => {
            state.resolved.raw.iterations =
                adjust_number(state.resolved.raw.iterations, delta, 1, 99)
        }
        8 => {
            state.resolved.raw.team_work = adjust_number(state.resolved.raw.team_work, delta, 0, 64)
        }
        9 => {
            let current = state
                .resolved
                .raw
                .codex_sub_agents
                .unwrap_or(state.resolved.raw.team_work);
            state.resolved.raw.codex_sub_agents = Some(adjust_number(current, delta, 0, 64));
        }
        10 => {
            let current = state
                .resolved
                .raw
                .claude_sub_agents
                .unwrap_or(state.resolved.raw.team_work);
            state.resolved.raw.claude_sub_agents = Some(adjust_number(current, delta, 0, 64));
        }
        11 => {
            let current = state
                .resolved
                .raw
                .gemini_sub_agents
                .unwrap_or(state.resolved.raw.team_work);
            state.resolved.raw.gemini_sub_agents = Some(adjust_number(current, delta, 0, 64));
        }
        12 => {
            state.resolved.raw.codex_sandbox = cycle_value(
                &state.resolved.raw.codex_sandbox,
                &["read-only", "workspace-write", "danger-full-access"],
                delta,
            )
        }
        13 => {
            state.resolved.raw.claude_permission_mode = cycle_value(
                &state.resolved.raw.claude_permission_mode,
                &[
                    "plan",
                    "default",
                    "acceptEdits",
                    "auto",
                    "dontAsk",
                    "bypassPermissions",
                ],
                delta,
            )
        }
        14 => {
            state.resolved.raw.codex_auth = cycle_value(
                &state.resolved.raw.codex_auth,
                &["auto", "social-login", "login", "api-key"],
                delta,
            )
        }
        15 => {
            state.resolved.raw.claude_auth = cycle_value(
                &state.resolved.raw.claude_auth,
                &["auto", "social-login", "oauth", "api-key", "keychain"],
                delta,
            )
        }
        16 => {
            state.resolved.raw.gemini_auth = cycle_value(
                &state.resolved.raw.gemini_auth,
                &["auto", "social-login", "login", "api-key"],
                delta,
            )
        }
        17 => {
            state.resolved.raw.codex_effort = cycle_optional(
                &state.resolved.raw.codex_effort,
                &["low", "medium", "high", "xhigh"],
                delta,
            )
        }
        18 => {
            state.resolved.raw.claude_effort = cycle_optional(
                &state.resolved.raw.claude_effort,
                &["low", "medium", "high", "xhigh", "max"],
                delta,
            )
        }
        19 => {
            state.resolved.raw.gemini_effort = cycle_optional(
                &state.resolved.raw.gemini_effort,
                &["low", "medium", "high"],
                delta,
            )
        }
        _ => {}
    }
    state.status = "Setting updated".to_string();
    Ok(())
}

fn adjust_capability(state: &mut StudioState, delta: isize) -> Result<(), String> {
    match state.capability_index {
        0 => {
            state.resolved.raw.codex_capabilities = cycle_value(
                &state.resolved.raw.codex_capabilities,
                &["inherit", "override"],
                delta,
            )
        }
        3 => {
            state.resolved.raw.claude_capabilities = cycle_value(
                &state.resolved.raw.claude_capabilities,
                &["inherit", "override"],
                delta,
            )
        }
        11 => {
            state.resolved.raw.claude_strict_mcp_config =
                !state.resolved.raw.claude_strict_mcp_config
        }
        12 => {
            state.resolved.raw.claude_disable_slash_commands =
                !state.resolved.raw.claude_disable_slash_commands
        }
        13 => {
            state.resolved.raw.gemini_capabilities = cycle_value(
                &state.resolved.raw.gemini_capabilities,
                &["inherit", "override"],
                delta,
            )
        }
        _ => {}
    }
    state.status = "Capability setting updated".to_string();
    Ok(())
}

fn adjust_linear(state: &mut StudioState, delta: isize) -> Result<(), String> {
    match state.linear_index {
        0 => {
            let current = if state.resolved.raw.linear_watch {
                "watch"
            } else if state.resolved.raw.deliver_linear {
                "deliver"
            } else {
                "off"
            };
            match cycle_value(current, &["off", "deliver", "watch"], delta).as_str() {
                "watch" => {
                    state.resolved.raw.deliver_linear = true;
                    state.resolved.raw.linear_watch = true;
                }
                "deliver" => {
                    state.resolved.raw.deliver_linear = true;
                    state.resolved.raw.linear_watch = false;
                }
                _ => {
                    state.resolved.raw.deliver_linear = false;
                    state.resolved.raw.linear_watch = false;
                }
            }
        }
        1 => {
            state.resolved.raw.linear_auth = cycle_value(
                &state.resolved.raw.linear_auth,
                &["api-key", "oauth"],
                delta,
            )
        }
        8 => {
            state.resolved.raw.linear_completion_gate = cycle_value(
                &state.resolved.raw.linear_completion_gate,
                &["delivered", "human-review", "ci-success", "review-or-ci"],
                delta,
            )
        }
        9 => {
            state.resolved.raw.linear_workspace_strategy = cycle_value(
                &state.resolved.raw.linear_workspace_strategy,
                &["worktree", "copy", "none"],
                delta,
            )
        }
        10 => {
            state.resolved.raw.linear_poll_interval = adjust_number(
                state.resolved.raw.linear_poll_interval as usize,
                delta,
                1,
                3600,
            ) as u64
        }
        11 => {
            state.resolved.raw.linear_max_attempts =
                adjust_number(state.resolved.raw.linear_max_attempts, delta, 1, 99)
        }
        12 => state.resolved.raw.no_linear_comments = !state.resolved.raw.no_linear_comments,
        13 => {
            state.resolved.raw.linear_update_review_state =
                !state.resolved.raw.linear_update_review_state
        }
        _ => {}
    }
    state.status = "Linear setting updated".to_string();
    Ok(())
}

const STUDIO_BG: Color = Color::Rgb(8, 10, 14);
const STUDIO_PANEL: Color = Color::Rgb(16, 20, 27);
const STUDIO_PANEL_ALT: Color = Color::Rgb(20, 25, 34);
const STUDIO_TEXT: Color = Color::Rgb(229, 232, 238);
const STUDIO_MUTED: Color = Color::Rgb(138, 148, 164);
const STUDIO_BORDER: Color = Color::Rgb(54, 65, 83);
const STUDIO_ACCENT: Color = Color::Rgb(71, 214, 181);
const STUDIO_GOLD: Color = Color::Rgb(246, 196, 83);
const STUDIO_PURPLE: Color = Color::Rgb(177, 139, 255);
const STUDIO_RED: Color = Color::Rgb(244, 97, 97);
const STUDIO_GREEN: Color = Color::Rgb(114, 222, 128);
const STUDIO_BLUE: Color = Color::Rgb(96, 165, 250);

fn draw(state: &StudioState) -> Result<(), String> {
    let backend = CrosstermBackend::new(io::stderr());
    let mut terminal = Terminal::new(backend)
        .map_err(|error| format!("Failed to open Studio terminal: {error}"))?;
    terminal
        .draw(|frame| render_studio(frame, state))
        .map_err(|error| format!("Failed to draw Studio: {error}"))?;
    Ok(())
}

fn configure_studio_color(raw: &CliArgs) {
    let color_enabled = !raw.no_color && raw.color != "never";
    force_color_output(color_enabled);
}

fn studio_clip(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let cleaned = text
        .chars()
        .map(|ch| {
            if ch == '\n' || ch == '\r' || ch == '\t' || ch.is_control() {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>();
    if cleaned.chars().count() <= max_chars {
        return cleaned;
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }
    let mut clipped = cleaned
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    clipped.push_str("...");
    clipped
}

fn render_studio(frame: &mut Frame<'_>, state: &StudioState) {
    let area = frame.area();
    frame.render_widget(Block::new().style(Style::default().bg(STUDIO_BG)), area);
    if area.width < 92 || area.height < 24 {
        render_compact_studio(frame, area, state);
        return;
    }

    let prompt_height = if state.input_mode.is_some() { 6 } else { 5 };
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(16),
            Constraint::Length(prompt_height),
            Constraint::Length(2),
        ])
        .split(area);

    render_header(frame, layout[0], state);

    if area.width >= 150 {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(31),
                Constraint::Min(64),
                Constraint::Length(46),
            ])
            .split(layout[1]);
        render_command_rail(frame, body[0], state);
        render_workbench(frame, body[1], state);
        render_configuration(frame, body[2], state);
    } else {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(29), Constraint::Min(60)])
            .split(layout[1]);
        render_command_rail(frame, body[0], state);
        render_medium_workbench(frame, body[1], state);
    }
    render_prompt(frame, layout[2], state);
    render_footer(frame, layout[3], state);
}

fn render_compact_studio(frame: &mut Frame<'_>, area: Rect, state: &StudioState) {
    let block = panel_block("Amon Hen Studio", true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = vec![
        Line::from(vec![Span::styled(
            "Terminal too small for full Studio",
            strong(STUDIO_GOLD),
        )]),
        Line::from("Resize wider/taller for the dashboard."),
        Line::from(format!(
            "Members: {} | lead {} | planner {}",
            state.resolved.members.join(","),
            state.resolved.raw.lead.as_deref().unwrap_or("auto"),
            state.resolved.raw.planner.as_deref().unwrap_or("none")
        )),
        Line::from(format!("Status: {}", state.status)),
        Line::from("Keys: r run | e prompt | Tab focus | Ctrl+C twice quit"),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &StudioState) {
    let workflow = build_workflow(&state.resolved);
    let total_tokens = total_session_tokens(state);
    let block = Block::new().style(Style::default().bg(STUDIO_BG));
    frame.render_widget(block, area);
    let title_line = Line::from(vec![
        Span::styled("Amon Hen", strong(STUDIO_GOLD)),
        Span::raw("  "),
        Span::styled("Rust-native agent command center", muted()),
        Span::raw("  "),
        status_span(&state.status),
    ]);
    let identity_chips = vec![
        chip("members", &state.resolved.members.join(","), STUDIO_ACCENT),
        Span::raw("  "),
        chip(
            "lead",
            state.resolved.raw.lead.as_deref().unwrap_or("auto"),
            STUDIO_PURPLE,
        ),
        Span::raw("  "),
        chip(
            "planner",
            state.resolved.raw.planner.as_deref().unwrap_or("none"),
            STUDIO_BLUE,
        ),
        Span::raw("  "),
        chip("handoff", on_off(state.resolved.raw.handoff), STUDIO_GREEN),
    ];
    let metric_chips = vec![
        chip(
            "iterations",
            &state.resolved.raw.iterations.to_string(),
            STUDIO_GOLD,
        ),
        Span::raw("  "),
        chip(
            "team",
            &team_chip_value(&workflow, area.width),
            STUDIO_ACCENT,
        ),
        Span::raw("  "),
        chip("tokens", &compact_count(total_tokens), STUDIO_PURPLE),
    ];
    let lines = if area.width < 132 {
        vec![
            title_line,
            Line::from(identity_chips),
            Line::from(metric_chips),
        ]
    } else {
        let mut chips = identity_chips;
        chips.push(Span::raw("  "));
        chips.extend(metric_chips);
        vec![title_line, Line::from(chips)]
    };
    let header = Paragraph::new(lines).style(Style::default().fg(STUDIO_TEXT).bg(STUDIO_BG));
    frame.render_widget(header, area);
}

fn render_command_rail(frame: &mut Frame<'_>, area: Rect, state: &StudioState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(21),
            Constraint::Length(8),
            Constraint::Length(7),
        ])
        .split(area);

    let items = MENU
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let selected = state.focus == Pane::Menu && index == state.menu_index;
            let style = if selected {
                selected_style()
            } else {
                Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL)
            };
            ListItem::new(Line::from(vec![
                Span::styled(if selected { "> " } else { "  " }, strong(STUDIO_ACCENT)),
                Span::styled(*item, style),
            ]))
            .style(style)
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(panel_block("Command rail", state.focus == Pane::Menu)),
        chunks[0],
    );

    let session = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Mode", muted()),
            Span::raw("  "),
            Span::styled("Studio", strong(STUDIO_ACCENT)),
        ]),
        Line::from(format!("Files tagged: {}", state.resolved.raw.files.len())),
        Line::from(format!("Commands: {}", state.resolved.raw.commands.len())),
        Line::from(format!(
            "Profile: {} ({} saved)",
            state.profile_name,
            state.profile_names.len()
        )),
        Line::from(format!("Timeout: {}s", state.resolved.raw.timeout)),
        Line::from(format!("Repo: {}", display_cwd(&state.resolved.cwd))),
        Line::from(format!("Config: {}", display_cwd(&state.profile_path))),
    ])
    .block(panel_block("Session", false))
    .style(Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL))
    .wrap(Wrap { trim: true });
    frame.render_widget(session, chunks[1]);

    let hints = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Tab", strong(STUDIO_GOLD)),
            Span::raw(" change focus"),
        ]),
        Line::from(vec![
            Span::styled("Enter", strong(STUDIO_GOLD)),
            Span::raw(" activate/edit"),
        ]),
        Line::from(vec![
            Span::styled("Left/Right", strong(STUDIO_GOLD)),
            Span::raw(" modify"),
        ]),
        Line::from(vec![
            Span::styled("r", strong(STUDIO_GOLD)),
            Span::raw(" run now"),
        ]),
        Line::from(vec![
            Span::styled("c", strong(STUDIO_GOLD)),
            Span::raw(" cancel job"),
        ]),
        Line::from(vec![
            Span::styled("e", strong(STUDIO_GOLD)),
            Span::raw(" edit prompt"),
        ]),
    ])
    .block(panel_block("Hotkeys", false))
    .style(Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL));
    frame.render_widget(hints, chunks[2]);
}

fn render_workbench(frame: &mut Frame<'_>, area: Rect, state: &StudioState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11),
            Constraint::Length(7),
            Constraint::Min(8),
        ])
        .split(area);
    render_provider_cards(frame, chunks[0], state);
    render_token_and_tools(frame, chunks[1], state);
    render_results_panel(frame, chunks[2], state);
}

fn render_medium_workbench(frame: &mut Frame<'_>, area: Rect, state: &StudioState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11),
            Constraint::Length(7),
            Constraint::Min(8),
        ])
        .split(area);
    render_provider_cards(frame, chunks[0], state);
    render_token_and_tools(frame, chunks[1], state);
    if chunks[2].width >= 86 {
        let lower = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(42), Constraint::Length(42)])
            .split(chunks[2]);
        render_results_panel(frame, lower[0], state);
        render_configuration(frame, lower[1], state);
    } else {
        let lower = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(chunks[2]);
        render_results_panel(frame, lower[0], state);
        render_configuration(frame, lower[1], state);
    }
}

fn render_provider_cards(frame: &mut Frame<'_>, area: Rect, state: &StudioState) {
    let members = if state.resolved.members.is_empty() {
        ENGINES
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    } else {
        state.resolved.members.clone()
    };
    let constraints = vec![Constraint::Ratio(1, members.len() as u32); members.len()];
    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    let max_tokens = members
        .iter()
        .map(|member| provider_live_token_usage(state, member).map_or(0, |usage| usage.total))
        .max()
        .unwrap_or(0)
        .max(1);

    for (index, member) in members.iter().enumerate() {
        let Some(area) = cards.get(index).copied() else {
            continue;
        };
        render_provider_card(frame, area, state, member, max_tokens);
    }
}

fn render_provider_card(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &StudioState,
    member: &str,
    max_tokens: usize,
) {
    let color = provider_color(member);
    let result = provider_result(state, member);
    let workflow = build_workflow(&state.resolved);
    let role = result
        .map(|result| result.role.clone())
        .unwrap_or_else(|| role_for(member, &workflow));
    let status = provider_status(state, member, result);
    let health = provider_health(state, member);
    let token_usage = provider_live_token_usage(state, member);
    let total_tokens = token_usage.map_or(0, |usage| usage.total);
    let percent = ((total_tokens.saturating_mul(100)) / max_tokens).min(100) as u16;
    let tools = provider_live_tool_count(state, member);
    let sub_agents = provider_live_sub_agent_count(state, member);
    let command = state
        .provider_detail
        .get(member)
        .map(|detail| studio_clip(detail, 64))
        .or_else(|| result.map(|result| studio_clip(&result.command, 64)))
        .unwrap_or_else(|| "not run yet".to_string());
    let block = panel_block(member.to_ascii_uppercase(), state.focus == Pane::Agents)
        .border_style(Style::default().fg(color))
        .title_style(strong(color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Length(2),
            Constraint::Min(1),
        ])
        .split(inner);
    let input = token_usage.map_or(0, |usage| usage.input);
    let output = token_usage.map_or(0, |usage| usage.output);
    let lines = vec![
        Line::from(vec![
            Span::styled("role ", muted()),
            Span::styled(role, strong(STUDIO_TEXT)),
            Span::raw("  "),
            Span::styled("status ", muted()),
            Span::styled(status, strong(status_color(status))),
        ]),
        Line::from(vec![
            Span::styled("auth ", muted()),
            Span::raw(health.auth_mode),
            Span::raw("  "),
            Span::styled("src ", muted()),
            Span::raw(studio_clip(&health.auth_source, 16)),
        ]),
        Line::from(vec![
            Span::styled("effort ", muted()),
            Span::raw(health.effort),
            Span::raw("  "),
            Span::styled("cap ", muted()),
            Span::raw(health.capability_mode),
        ]),
        Line::from(vec![
            Span::styled("model ", muted()),
            Span::raw(studio_clip(&health.model, 28)),
        ]),
        Line::from(vec![
            Span::styled("bin ", muted()),
            Span::styled(
                health.binary_status,
                strong(status_color(health.binary_status)),
            ),
            Span::raw("  "),
            Span::raw(studio_clip(&health.binary, 28)),
        ]),
        Line::from(vec![
            Span::styled("in ", muted()),
            Span::raw(compact_count(input)),
            Span::raw("  "),
            Span::styled("out ", muted()),
            Span::raw(compact_count(output)),
            Span::raw("  "),
            Span::styled("tools ", muted()),
            Span::raw(tools.to_string()),
            Span::raw("  "),
            Span::styled("subs ", muted()),
            Span::raw(sub_agents.to_string()),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL))
            .wrap(Wrap { trim: true }),
        chunks[0],
    );
    frame.render_widget(
        Gauge::default()
            .ratio(f64::from(percent) / 100.0)
            .label(Span::styled(
                format!("{} tokens", compact_count(total_tokens)),
                strong(STUDIO_TEXT),
            ))
            .gauge_style(Style::default().fg(color).bg(STUDIO_PANEL_ALT)),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(command)
            .style(Style::default().fg(STUDIO_MUTED).bg(STUDIO_PANEL))
            .wrap(Wrap { trim: true }),
        chunks[2],
    );
}

fn render_token_and_tools(frame: &mut Frame<'_>, area: Rect, state: &StudioState) {
    let mut rows = state
        .resolved
        .members
        .iter()
        .map(|member| {
            let result = provider_result(state, member);
            let token_usage = provider_live_token_usage(state, member);
            Row::new(vec![
                member.to_string(),
                provider_status(state, member, result).to_string(),
                token_usage.map_or("0".to_string(), |usage| compact_count(usage.input)),
                token_usage.map_or("0".to_string(), |usage| compact_count(usage.output)),
                token_usage.map_or("0".to_string(), |usage| compact_count(usage.total)),
                provider_live_tool_count(state, member).to_string(),
                provider_live_sub_agent_count(state, member).to_string(),
            ])
            .style(Style::default().fg(provider_color(member)).bg(STUDIO_PANEL))
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        rows.push(Row::new(vec!["none", "ready", "0", "0", "0", "0", "0"]));
    }
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(9),
            Constraint::Length(7),
            Constraint::Length(7),
        ],
    )
    .header(
        Row::new(vec![
            "agent", "status", "input", "output", "total", "tools", "subs",
        ])
        .style(strong(STUDIO_MUTED)),
    )
    .block(panel_block(
        "Token usage / tools",
        state.focus == Pane::Agents,
    ))
    .style(Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL));
    frame.render_widget(table, area);
}

fn render_results_panel(frame: &mut Frame<'_>, area: Rect, state: &StudioState) {
    let raw_lines = result_lines(state);
    let available = area.height.saturating_sub(2) as usize;
    let visible = visible_lines(&raw_lines, state.result_index, available);
    let lines = visible
        .into_iter()
        .map(|(index, line)| {
            let style = if state.focus == Pane::Results && index == state.result_index {
                strong(STUDIO_ACCENT)
            } else if line.contains("[ok]") {
                strong(STUDIO_GREEN)
            } else if line.contains("[err]") || line.contains("failed") {
                strong(STUDIO_RED)
            } else {
                Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL)
            };
            Line::from(Span::styled(line, style))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel_block(
                "Results and execution log",
                state.focus == Pane::Results,
            ))
            .style(Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_configuration(frame: &mut Frame<'_>, area: Rect, state: &StudioState) {
    let active = matches!(
        state.focus,
        Pane::Settings | Pane::Capabilities | Pane::Linear
    );
    let block = panel_block("Configure on the go", active);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(if state.show_help { 8 } else { 4 }),
        ])
        .split(inner);
    let tab_index = match state.focus {
        Pane::Capabilities => 1,
        Pane::Linear => 2,
        _ => 0,
    };
    frame.render_widget(
        Tabs::new(vec!["Settings", "Capabilities", "Linear"])
            .select(tab_index)
            .style(Style::default().fg(STUDIO_MUTED).bg(STUDIO_PANEL))
            .highlight_style(strong(STUDIO_ACCENT))
            .divider(Span::styled("/", muted())),
        chunks[0],
    );

    let (lines, selected, config_active) = match tab_index {
        1 => (
            capability_lines(state),
            state.capability_index,
            state.focus == Pane::Capabilities,
        ),
        2 => (
            linear_lines(state),
            state.linear_index,
            state.focus == Pane::Linear,
        ),
        _ => (
            settings_lines(state),
            state.setting_index,
            state.focus == Pane::Settings,
        ),
    };
    let available = chunks[1].height as usize;
    let items = visible_lines(&lines, selected, available)
        .into_iter()
        .map(|(index, line)| config_list_item(line, config_active && index == selected))
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).style(Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL)),
        chunks[1],
    );

    let help = if state.show_help {
        vec![
            Line::from("Tab cycles panels. Up/Down selects."),
            Line::from("Left/Right changes toggles and numeric values."),
            Line::from("Enter edits paths, lists, prompts, and Linear filters."),
            Line::from("r runs, c cancels the active job, e edits prompt."),
            Line::from("? toggles help."),
            Line::from("Ctrl+C twice exits without surprise."),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled("Tip", strong(STUDIO_GOLD)),
                Span::raw(" press ? for help"),
            ]),
            Line::from("Enter edits values. Arrows modify live."),
        ]
    };
    frame.render_widget(
        Paragraph::new(help)
            .style(Style::default().fg(STUDIO_MUTED).bg(STUDIO_PANEL_ALT))
            .wrap(Wrap { trim: true })
            .block(Block::new().style(Style::default().bg(STUDIO_PANEL_ALT))),
        chunks[2],
    );
}

fn render_prompt(frame: &mut Frame<'_>, area: Rect, state: &StudioState) {
    let editing = state.input_mode.is_some();
    let block = panel_block(if editing { "Editing" } else { "Prompt" }, editing);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let context = format!(
        "files:{} commands:{} cwd:{}",
        state.resolved.raw.files.len(),
        state.resolved.raw.commands.len(),
        display_cwd(&state.resolved.cwd)
    );
    let mut lines = vec![Line::from(vec![Span::styled(context, muted())])];
    if let Some(mode) = &state.input_mode {
        lines.push(Line::from(vec![
            Span::styled(format!("{mode:?}: "), strong(STUDIO_GOLD)),
            Span::styled(format!("{}_", state.input_buffer), strong(STUDIO_TEXT)),
        ]));
        lines.push(Line::from("Enter saves. Esc cancels."));
    } else {
        lines.push(Line::from(if state.prompt.trim().is_empty() {
            "(empty prompt)".to_string()
        } else {
            state.prompt.trim().to_string()
        }));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, state: &StudioState) {
    let focus = match state.focus {
        Pane::Menu => "menu",
        Pane::Settings => "settings",
        Pane::Agents => "agents",
        Pane::Capabilities => "capabilities",
        Pane::Linear => "linear",
        Pane::Results => "results",
    };
    let line = Line::from(vec![
        Span::styled("focus ", muted()),
        Span::styled(focus, strong(STUDIO_ACCENT)),
        Span::raw("   "),
        Span::styled("r", strong(STUDIO_GOLD)),
        Span::raw(" run  "),
        Span::styled("c", strong(STUDIO_GOLD)),
        Span::raw(" cancel  "),
        Span::styled("e", strong(STUDIO_GOLD)),
        Span::raw(" prompt  "),
        Span::styled("Tab", strong(STUDIO_GOLD)),
        Span::raw(" focus  "),
        Span::styled("Enter", strong(STUDIO_GOLD)),
        Span::raw(" edit/activate  "),
        Span::styled("?", strong(STUDIO_GOLD)),
        Span::raw(" help  "),
        Span::styled("Ctrl+C twice", strong(STUDIO_GOLD)),
        Span::raw(" quit"),
    ]);
    frame.render_widget(
        Paragraph::new(line)
            .alignment(Alignment::Center)
            .style(Style::default().fg(STUDIO_TEXT).bg(STUDIO_BG)),
        area,
    );
}

fn panel_block<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    let border = if focused {
        STUDIO_ACCENT
    } else {
        STUDIO_BORDER
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(title)
        .title_style(strong(if focused { STUDIO_ACCENT } else { STUDIO_MUTED }))
        .border_style(Style::default().fg(border))
        .style(Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL))
}

fn strong(color: Color) -> Style {
    Style::default()
        .fg(color)
        .bg(STUDIO_PANEL)
        .add_modifier(Modifier::BOLD)
}

fn muted() -> Style {
    Style::default().fg(STUDIO_MUTED).bg(STUDIO_PANEL)
}

fn selected_style() -> Style {
    Style::default()
        .fg(STUDIO_BG)
        .bg(STUDIO_ACCENT)
        .add_modifier(Modifier::BOLD)
}

fn status_span(status: &str) -> Span<'static> {
    let color = if status.contains("failed") || status.contains("attention") {
        STUDIO_RED
    } else if status.contains("completed") || status.contains("Ready") {
        STUDIO_GREEN
    } else {
        STUDIO_GOLD
    };
    Span::styled(format!(" status: {status} "), strong(color))
}

fn chip(label: &'static str, value: &str, color: Color) -> Span<'static> {
    Span::styled(format!(" {label}:{value} "), strong(color))
}

fn provider_color(member: &str) -> Color {
    match member {
        "codex" => STUDIO_BLUE,
        "claude" => STUDIO_PURPLE,
        "gemini" => STUDIO_GOLD,
        _ => STUDIO_ACCENT,
    }
}

fn team_chip_value(workflow: &Workflow, width: u16) -> String {
    if width < 132 {
        let total = workflow.teams.values().sum::<usize>();
        return format!("{total} sub-agents");
    }
    workflow
        .teams
        .iter()
        .map(|(name, size)| format!("{name}:{size}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn status_color(status: &str) -> Color {
    match status {
        "ok" => STUDIO_GREEN,
        "err" | "error" | "timeout" | "missing" => STUDIO_RED,
        "running" | "queued" => STUDIO_GOLD,
        "ready" => STUDIO_MUTED,
        _ => STUDIO_GOLD,
    }
}

fn provider_status<'a>(
    state: &'a StudioState,
    member: &str,
    result: Option<&'a EngineResult>,
) -> &'a str {
    state
        .provider_status
        .get(member)
        .map(String::as_str)
        .or_else(|| result.map(|result| result.status.as_str()))
        .unwrap_or("ready")
}

fn provider_result<'a>(state: &'a StudioState, member: &str) -> Option<&'a EngineResult> {
    state
        .last_result
        .as_ref()?
        .members
        .iter()
        .find(|result| result.name == member)
}

fn provider_live_token_usage<'a>(state: &'a StudioState, member: &str) -> Option<&'a TokenUsage> {
    provider_result(state, member)
        .map(|result| &result.token_usage)
        .or_else(|| state.live_token_usage.get(member))
}

fn provider_live_tool_count(state: &StudioState, member: &str) -> usize {
    provider_result(state, member)
        .map(|result| result.tool_calls.len())
        .unwrap_or_else(|| *state.live_tool_counts.get(member).unwrap_or(&0))
}

fn provider_live_sub_agent_count(state: &StudioState, member: &str) -> usize {
    provider_result(state, member)
        .map(|result| result.sub_agents.len())
        .unwrap_or_else(|| state.live_sub_agents.get(member).map_or(0, HashSet::len))
}

fn provider_effort(state: &StudioState, member: &str) -> String {
    let value = match member {
        "codex" => state.resolved.raw.codex_effort.as_deref(),
        "claude" => state.resolved.raw.claude_effort.as_deref(),
        "gemini" => state.resolved.raw.gemini_effort.as_deref(),
        _ => None,
    };
    value.unwrap_or("default").to_string()
}

fn provider_model(state: &StudioState, member: &str) -> String {
    let value = match member {
        "codex" => state.resolved.raw.codex_model.as_deref(),
        "claude" => state.resolved.raw.claude_model.as_deref(),
        "gemini" => state.resolved.raw.gemini_model.as_deref(),
        _ => None,
    };
    value.unwrap_or("default").to_string()
}

struct ProviderHealth {
    binary: String,
    binary_status: &'static str,
    auth_mode: String,
    auth_source: String,
    model: String,
    effort: String,
    capability_mode: String,
}

fn provider_health(state: &StudioState, member: &str) -> ProviderHealth {
    let binary = resolve_binary(member);
    let binary_status = if command_available(&binary) {
        "ok"
    } else {
        "missing"
    };
    ProviderHealth {
        binary,
        binary_status,
        auth_mode: provider_auth(&state.resolved, member),
        auth_source: provider_auth_source(state, member),
        model: provider_model(state, member),
        effort: provider_effort(state, member),
        capability_mode: provider_capability(&state.resolved, member).mode,
    }
}

fn provider_auth_source(state: &StudioState, member: &str) -> String {
    let auth = provider_auth(&state.resolved, member);
    match member {
        "codex" if auth == "api-key" => env_source("OPENAI_API_KEY"),
        "claude" if auth == "api-key" => env_source("ANTHROPIC_API_KEY"),
        "gemini" if auth == "api-key" => env_source("GEMINI_API_KEY"),
        "codex" => auth_local_source(&auth, "codex cli"),
        "claude" => auth_local_source(&auth, "claude cli"),
        "gemini" => auth_local_source(&auth, "gemini cli"),
        _ => "unknown".to_string(),
    }
}

fn env_source(name: &str) -> String {
    if std::env::var(name)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
    {
        format!("env:{name}")
    } else {
        format!("env:{name} missing")
    }
}

fn auth_local_source(auth: &str, source: &str) -> String {
    match auth {
        "auto" => "auto".to_string(),
        "social-login" | "login" | "oauth" | "keychain" => source.to_string(),
        value => value.to_string(),
    }
}

#[cfg(test)]
fn provider_health_lines(state: &StudioState) -> Vec<String> {
    state
        .resolved
        .members
        .iter()
        .map(|member| {
            let health = provider_health(state, member);
            format!(
                "{} bin:{} {} auth:{}/{} model:{} effort:{} cap:{}",
                member,
                health.binary_status,
                health.binary,
                health.auth_mode,
                health.auth_source,
                health.model,
                health.effort,
                health.capability_mode
            )
        })
        .collect()
}

fn total_session_tokens(state: &StudioState) -> usize {
    let Some(result) = &state.last_result else {
        return 0;
    };
    result
        .members
        .iter()
        .map(|member| member.token_usage.total)
        .sum::<usize>()
        + result.summary.token_usage.total
}

fn compact_count(value: usize) -> String {
    if value >= 1_000_000 {
        format!("{:.1}m", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn display_cwd(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

fn visible_lines(lines: &[String], selected: usize, available_rows: usize) -> Vec<(usize, String)> {
    if lines.is_empty() || available_rows == 0 {
        return Vec::new();
    }
    let selected = selected.min(lines.len().saturating_sub(1));
    let window = available_rows.min(lines.len());
    let start = selected
        .saturating_sub(window / 2)
        .min(lines.len().saturating_sub(window));
    lines
        .iter()
        .enumerate()
        .skip(start)
        .take(window)
        .map(|(index, line)| (index, line.clone()))
        .collect()
}

fn config_list_item(line: String, selected: bool) -> ListItem<'static> {
    let clean = line.trim_start_matches('>').trim_start().to_string();
    let style = if selected {
        selected_style()
    } else {
        Style::default().fg(STUDIO_TEXT).bg(STUDIO_PANEL)
    };
    if let Some((label, value)) = clean.split_once(':') {
        ListItem::new(Line::from(vec![
            Span::styled(if selected { "> " } else { "  " }, strong(STUDIO_ACCENT)),
            Span::styled(
                label.to_string(),
                strong(if selected { STUDIO_BG } else { STUDIO_TEXT }),
            ),
            Span::styled(": ", style),
            Span::styled(value.trim().to_string(), style),
        ]))
        .style(style)
    } else {
        ListItem::new(Line::from(vec![
            Span::styled(if selected { "> " } else { "  " }, strong(STUDIO_ACCENT)),
            Span::styled(clean, style),
        ]))
        .style(style)
    }
}

fn settings_lines(state: &StudioState) -> Vec<String> {
    select_lines(
        state.focus == Pane::Settings,
        state.setting_index,
        vec![
            format!("Handoff: {}", on_off(state.resolved.raw.handoff)),
            format!(
                "Lead: {}",
                state.resolved.raw.lead.as_deref().unwrap_or("auto")
            ),
            format!(
                "Planner: {}",
                state.resolved.raw.planner.as_deref().unwrap_or("none")
            ),
            format!("Summarizer: {}", state.resolved.raw.summarizer),
            format!("Codex model: {}", opt(&state.resolved.raw.codex_model)),
            format!("Claude model: {}", opt(&state.resolved.raw.claude_model)),
            format!("Gemini model: {}", opt(&state.resolved.raw.gemini_model)),
            format!("Iterations: {}", state.resolved.raw.iterations),
            format!("Team default: {}", state.resolved.raw.team_work),
            format!(
                "Codex sub-agents: {}",
                state
                    .resolved
                    .raw
                    .codex_sub_agents
                    .unwrap_or(state.resolved.raw.team_work)
            ),
            format!(
                "Claude sub-agents: {}",
                state
                    .resolved
                    .raw
                    .claude_sub_agents
                    .unwrap_or(state.resolved.raw.team_work)
            ),
            format!(
                "Gemini sub-agents: {}",
                state
                    .resolved
                    .raw
                    .gemini_sub_agents
                    .unwrap_or(state.resolved.raw.team_work)
            ),
            format!("Codex sandbox: {}", state.resolved.raw.codex_sandbox),
            format!(
                "Claude permission: {}",
                state.resolved.raw.claude_permission_mode
            ),
            format!("Codex auth: {}", state.resolved.raw.codex_auth),
            format!("Claude auth: {}", state.resolved.raw.claude_auth),
            format!("Gemini auth: {}", state.resolved.raw.gemini_auth),
            format!("Codex effort: {}", opt(&state.resolved.raw.codex_effort)),
            format!("Claude effort: {}", opt(&state.resolved.raw.claude_effort)),
            format!("Gemini effort: {}", opt(&state.resolved.raw.gemini_effort)),
        ],
    )
}

fn capability_lines(state: &StudioState) -> Vec<String> {
    let mut lines = select_lines(
        state.focus == Pane::Capabilities,
        state.capability_index,
        vec![
            format!("Codex mode: {}", state.resolved.raw.codex_capabilities),
            format!("Codex config: {}", list(&state.resolved.raw.codex_config)),
            format!(
                "Codex MCP profile: {}",
                state
                    .resolved
                    .raw
                    .codex_mcp_profile
                    .as_deref()
                    .unwrap_or("none")
            ),
            format!("Claude mode: {}", state.resolved.raw.claude_capabilities),
            format!(
                "Claude MCP: {}",
                list(&state.resolved.raw.claude_mcp_config)
            ),
            format!(
                "Claude allowed: {}",
                list(&state.resolved.raw.claude_allowed_tools)
            ),
            format!(
                "Claude disallowed: {}",
                list(&state.resolved.raw.claude_disallowed_tools)
            ),
            format!("Claude tools: {}", list(&state.resolved.raw.claude_tools)),
            format!(
                "Claude agent: {}",
                state.resolved.raw.claude_agent.as_deref().unwrap_or("none")
            ),
            format!(
                "Claude agents JSON: {}",
                state
                    .resolved
                    .raw
                    .claude_agents_json
                    .as_deref()
                    .map(|value| studio_clip(value, 32))
                    .unwrap_or_else(|| "none".to_string())
            ),
            format!(
                "Claude plugin dirs: {}",
                list(&state.resolved.raw.claude_plugin_dir)
            ),
            format!(
                "Claude strict MCP: {}",
                on_off(state.resolved.raw.claude_strict_mcp_config)
            ),
            format!(
                "Claude slash skills off: {}",
                on_off(state.resolved.raw.claude_disable_slash_commands)
            ),
            format!("Gemini mode: {}", state.resolved.raw.gemini_capabilities),
            format!(
                "Gemini settings: {}",
                state
                    .resolved
                    .raw
                    .gemini_settings
                    .as_deref()
                    .unwrap_or("none")
            ),
            format!(
                "Gemini tools: {}",
                list(&state.resolved.raw.gemini_tools_profile)
            ),
            format!(
                "Gemini MCP allow: {}",
                list(&state.resolved.raw.gemini_allowed_mcp_servers)
            ),
            format!("Gemini policy: {}", list(&state.resolved.raw.gemini_policy)),
            format!(
                "Gemini admin policy: {}",
                list(&state.resolved.raw.gemini_admin_policy)
            ),
        ],
    );
    if let Some(status) = &state.last_capability_result {
        lines.push(String::new());
        lines.extend(status.lines().take(8).map(ToString::to_string));
    }
    lines
}

fn linear_lines(state: &StudioState) -> Vec<String> {
    let mode = if state.resolved.raw.linear_watch {
        "watch"
    } else if state.resolved.raw.deliver_linear {
        "deliver"
    } else {
        "off"
    };
    let mut lines = select_lines(
        state.focus == Pane::Linear,
        state.linear_index,
        vec![
            format!("Mode: {mode}"),
            format!("Auth: {}", state.resolved.raw.linear_auth),
            format!("Issues: {}", list(&state.resolved.raw.linear_issue)),
            format!(
                "Query: {}",
                state.resolved.raw.linear_query.as_deref().unwrap_or("none")
            ),
            format!("Projects: {}", list(&state.resolved.raw.linear_project)),
            format!("Epics: {}", list(&state.resolved.raw.linear_epic)),
            format!(
                "Team: {}",
                state.resolved.raw.linear_team.as_deref().unwrap_or("any")
            ),
            format!(
                "State: {}",
                state.resolved.raw.linear_state.as_deref().unwrap_or("any")
            ),
            format!("Gate: {}", state.resolved.raw.linear_completion_gate),
            format!(
                "Workspace: {}",
                state.resolved.raw.linear_workspace_strategy
            ),
            format!(
                "Poll interval: {}s",
                state.resolved.raw.linear_poll_interval
            ),
            format!("Max attempts: {}", state.resolved.raw.linear_max_attempts),
            format!(
                "Comments: {}",
                if state.resolved.raw.no_linear_comments {
                    "off"
                } else {
                    "on"
                }
            ),
            format!(
                "Update review state: {}",
                on_off(state.resolved.raw.linear_update_review_state)
            ),
            format!(
                "Attach media: {}",
                list(&state.resolved.raw.linear_attach_media)
            ),
            "Refresh status".to_string(),
            "Deliver now".to_string(),
        ],
    );
    if let Some(result) = &state.last_linear_result {
        lines.extend(result.lines().take(6).map(ToString::to_string));
    }
    lines
}

fn result_lines(state: &StudioState) -> Vec<String> {
    let mut lines = Vec::new();
    if !state.run_events.is_empty() {
        lines.push("Live run log".to_string());
        lines.extend(state.run_events.iter().cloned());
        lines.push(String::new());
    }
    if let Some(auth) = &state.last_auth_result {
        lines.push("Auth status".to_string());
        lines.extend(auth.lines().take(8).map(ToString::to_string));
        lines.push(String::new());
    }
    let Some(result) = &state.last_result else {
        if lines.is_empty() {
            lines.push("No run yet".to_string());
        }
        return lines;
    };
    lines.extend(result.members.iter().map(|member| {
        format!(
            "{} [{}] role:{} tokens:{} tools:{} sub-agents:{}",
            member.name,
            member.status,
            member.role,
            member.token_usage.total,
            member.tool_calls.len(),
            member.sub_agents.len()
        )
    }));
    for member in &result.members {
        if !member.detail.trim().is_empty() {
            lines.push(format!("{} detail: {}", member.name, member.detail));
        }
        for sub_agent in &member.sub_agents {
            lines.push(format!(
                "  {} [{}] tokens:{} tools:{}{}",
                sub_agent.role,
                sub_agent.status,
                sub_agent.token_usage.total,
                sub_agent.tool_calls.len(),
                if sub_agent.detail.trim().is_empty() {
                    String::new()
                } else {
                    format!(" detail: {}", studio_clip(&sub_agent.detail, 100))
                }
            ));
        }
    }
    for command in &result.prompt_commands {
        lines.push(format!(
            "cmd [{}] {}",
            command.status,
            studio_clip(&command.command, 54)
        ));
    }
    for member in &result.members {
        if !member.tool_calls.is_empty() || !member.sub_agents.is_empty() {
            lines.push(format!(
                "{} telemetry tools:{} sub-agents:{}",
                member.name,
                member.tool_calls.len(),
                member.sub_agents.len()
            ));
        }
    }
    lines.push(format!(
        "Synthesis [{}] via {} tokens:{}",
        result.summary.status, result.summary.name, result.summary.token_usage.total
    ));
    if !result.summary.output.trim().is_empty() {
        lines.push(String::new());
        lines.extend(
            result
                .summary
                .output
                .lines()
                .take(10)
                .map(ToString::to_string),
        );
    } else if !result.summary.detail.trim().is_empty() {
        lines.push(result.summary.detail.clone());
    }
    lines
}

fn render_noninteractive_studio_snapshot(resolved: &ResolvedArgs) -> String {
    [
        "Amon Hen Studio requires an interactive TTY.",
        "Native Studio is available from a terminal with --studio.",
        "",
        "Current setup:",
        &format!("- members: {}", resolved.members.join(",")),
        &format!("- lead: {}", resolved.raw.lead.as_deref().unwrap_or("auto")),
        &format!(
            "- planner: {}",
            resolved.raw.planner.as_deref().unwrap_or("none")
        ),
        &format!("- iterations: {}", resolved.raw.iterations),
        &format!("- handoff: {}", on_off(resolved.raw.handoff)),
    ]
    .join("\n")
}

fn select_lines(focused: bool, selected: usize, lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            format!(
                "{} {}",
                if focused && index == selected {
                    ">"
                } else {
                    " "
                },
                line
            )
        })
        .collect()
}

fn wrap_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    let len = len as isize;
    (current as isize + delta).rem_euclid(len) as usize
}

fn adjust_number(value: usize, delta: isize, min: usize, max: usize) -> usize {
    (value as isize + delta).clamp(min as isize, max as isize) as usize
}

fn cycle_value(current: &str, values: &[&str], delta: isize) -> String {
    let index = values
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    values[wrap_index(index, values.len(), delta)].to_string()
}

fn cycle_optional(current: &Option<String>, values: &[&str], delta: isize) -> Option<String> {
    let mut all = vec!["none"];
    all.extend(values.iter().copied());
    let current = current.as_deref().unwrap_or("none");
    let next = cycle_value(current, &all, delta);
    (next != "none").then_some(next)
}

fn cycle_optional_engine(
    current: Option<&str>,
    members: &[String],
    delta: isize,
) -> Option<String> {
    let mut values = vec!["none".to_string()];
    values.extend(members.iter().cloned());
    let index = values
        .iter()
        .position(|value| current == Some(value.as_str()))
        .unwrap_or(0);
    let next = values[wrap_index(index, values.len(), delta)].clone();
    (next != "none").then_some(next)
}

fn cycle_summarizer(current: &str, delta: isize) -> String {
    cycle_value(current, &["auto", "codex", "claude", "gemini"], delta)
}

fn settings_len() -> usize {
    20
}

fn capabilities_len() -> usize {
    19
}

fn linear_len() -> usize {
    17
}

fn result_len(state: &StudioState) -> usize {
    let result_lines = state
        .last_result
        .as_ref()
        .map(|result| result.members.len() + 2)
        .unwrap_or(1);
    result_lines + state.run_events.len()
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn opt(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("none")
}

fn list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn cycles_values_wrapping() {
        assert_eq!(cycle_value("auto", &["auto", "oauth"], 1), "oauth");
        assert_eq!(cycle_value("auto", &["auto", "oauth"], -1), "oauth");
    }

    #[test]
    fn renders_noninteractive_snapshot() {
        let args = CliArgs::try_parse_from(["amon-hen", "--studio", "hello"]).unwrap();
        let resolved = resolve_args(args).unwrap();
        let snapshot = render_noninteractive_studio_snapshot(&resolved);
        assert!(snapshot.contains("Native Studio is available"));
        assert!(snapshot.contains("members: codex,claude,gemini"));
    }

    #[test]
    fn studio_clip_removes_embedded_newlines() {
        let clipped = studio_clip("Claude agents JSON\n...[truncated]\tvalue", 24);

        assert!(!clipped.contains('\n'));
        assert!(!clipped.contains('\t'));
        assert!(clipped.chars().count() <= 24);
    }

    #[test]
    fn dashboard_renders_telemetry_configuration_and_color() {
        let mut state = test_state("Inspect this repo and suggest the cleanest next patch");
        state.last_result = Some(test_result(&state));

        let (rendered, has_accent) = render_to_string(&state, 180, 46);

        assert!(rendered.contains("Amon Hen"));
        assert!(rendered.contains("Command rail"));
        assert!(rendered.contains("Token usage / tools"));
        assert!(rendered.contains("Configure on the go"));
        assert!(rendered.contains("Results and execution log"));
        assert!(rendered.contains("model"));
        assert!(rendered.contains("1.5k"));
        assert!(rendered.contains("cargo test"));
        assert!(has_accent, "dashboard should render styled/colorized cells");
    }

    #[test]
    fn medium_dashboard_keeps_configuration_visible() {
        let mut state = test_state("Inspect this repo");
        state.focus = Pane::Settings;

        let (rendered, has_accent) = render_to_string(&state, 120, 34);

        assert!(rendered.contains("Command rail"));
        assert!(rendered.contains("Token usage / tools"));
        assert!(rendered.contains("Configure on the go"));
        assert!(rendered.contains("Codex model"));
        assert!(has_accent, "medium dashboard should keep colored styling");
    }

    #[test]
    fn compact_dashboard_stays_useful_on_small_terminals() {
        let state = test_state("hello");

        let (rendered, _) = render_to_string(&state, 80, 18);

        assert!(rendered.contains("Terminal too small"));
        assert!(rendered.contains("Members:"));
    }

    #[test]
    fn ctrl_c_requires_second_press_to_quit() {
        let mut state = test_state("hello");
        let ctrl_c = || Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

        let first = handle_event(&mut state, ctrl_c()).unwrap();
        assert!(matches!(first, StudioAction::None));
        assert!(state.status.contains("again"));

        let second = handle_event(&mut state, ctrl_c()).unwrap();
        assert!(matches!(second, StudioAction::Quit));
    }

    #[test]
    fn raw_etx_counts_as_second_ctrl_c() {
        let mut state = test_state("hello");
        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        let raw_etx = Event::Key(KeyEvent::new(KeyCode::Char('\u{3}'), KeyModifiers::NONE));

        let first = handle_event(&mut state, ctrl_c).unwrap();
        assert!(matches!(first, StudioAction::None));

        let second = handle_event(&mut state, raw_etx).unwrap();
        assert!(matches!(second, StudioAction::Quit));
    }

    #[test]
    fn progress_events_update_dashboard_without_leaving_studio() {
        let mut state = test_state("hello");
        apply_progress_event(
            &mut state,
            progress_event(
                Some("codex"),
                Some("planner"),
                ProgressStage::Spawn,
                Some("running"),
                "[amon-hen] spawn codex planner iteration 1/1",
            ),
        );
        apply_progress_event(
            &mut state,
            progress_event_with_context(
                Some("codex"),
                Some("planner:sub-agent-1"),
                ProgressStage::Heartbeat,
                Some("streaming"),
                Some(1),
                Some(1),
                true,
                None,
                Some(TokenUsage {
                    input: 100,
                    output: 23,
                    total: 123,
                    estimated: true,
                    source: "test".to_string(),
                }),
                vec![ToolUsage {
                    name: "Bash".to_string(),
                    kind: "tool".to_string(),
                    status: "running".to_string(),
                    detail: "git status".to_string(),
                }],
                "[amon-hen] stream codex planner:sub-agent-1 stdout: visible work",
            ),
        );

        assert_eq!(
            state.provider_status.get("codex").map(String::as_str),
            Some("streaming")
        );
        let (rendered, _) = render_to_string(&state, 180, 46);
        assert!(rendered.contains("Live run log"));
        assert!(rendered.contains("spawn codex"));
        assert!(rendered.contains("streaming"));
        assert!(rendered.contains("123"));
        assert!(rendered.contains("visible work"));
    }

    #[test]
    fn studio_actions_are_dashboard_jobs() {
        let actions = [
            (StudioAction::RunAmonHen, StudioJobKind::AmonHen),
            (StudioAction::SocialLogin, StudioJobKind::SocialLogin),
            (StudioAction::AuthStatus, StudioJobKind::AuthStatus),
            (
                StudioAction::CapabilitiesStatus,
                StudioJobKind::CapabilitiesStatus,
            ),
            (StudioAction::LinearStatus, StudioJobKind::LinearStatus),
            (StudioAction::LinearDeliver, StudioJobKind::LinearDeliver),
        ];

        for (action, kind) in actions {
            assert_eq!(dashboard_job_kind(&action), Some(kind));
        }
        assert_eq!(dashboard_job_kind(&StudioAction::Quit), None);
    }

    #[test]
    fn cancel_hotkey_marks_active_job_cancelled() {
        let mut state = test_state("hello");
        let (_tx, rx) = mpsc::channel();
        state.run_job = Some(StudioRunJob {
            rx,
            started: Instant::now(),
            cancel: Arc::new(AtomicBool::new(false)),
            kind: StudioJobKind::AuthStatus,
        });

        let action = handle_event(
            &mut state,
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
        )
        .unwrap();

        assert!(matches!(action, StudioAction::CancelJob));
        cancel_studio_job(&mut state);
        let job = state.run_job.as_ref().unwrap();
        assert!(job.cancel.load(Ordering::Relaxed));
        assert!(state.status.contains("cancellation requested"));
        assert!(state
            .run_events
            .iter()
            .any(|line| line.contains("cancellation")));
    }

    #[test]
    fn profile_save_load_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = test_state("first prompt");
        state.profile_path = temp.path().join("studio-profiles.json");
        state.profile_name = "roundtrip".to_string();
        state.resolved.raw.codex_model = Some("profile-model".to_string());
        state.resolved.raw.linear_issue = vec!["ENG-123".to_string()];
        state.resolved.raw.linear_until_complete = true;
        state.resolved.raw.linear_limit = 7;
        state.resolved.raw.linear_max_concurrency = 3;
        state.resolved.raw.linear_workflow_file = Some(PathBuf::from("docs/workflow.md"));
        state.resolved.raw.delivery_phases = vec!["plan".to_string(), "verify".to_string()];
        state.resolved.raw.codex_capabilities = "override".to_string();

        save_studio_profile(&mut state, "roundtrip").unwrap();

        state.prompt = "changed prompt".to_string();
        state.resolved.raw.codex_model = Some("changed-model".to_string());
        state.resolved.raw.linear_issue.clear();
        state.resolved.raw.linear_until_complete = false;
        state.resolved.raw.linear_limit = 1;
        state.resolved.raw.linear_max_concurrency = 1;
        state.resolved.raw.linear_workflow_file = None;
        state.resolved.raw.delivery_phases.clear();
        state.resolved.raw.codex_capabilities = "inherit".to_string();
        load_and_apply_studio_profile(&mut state, "roundtrip").unwrap();

        assert_eq!(state.prompt, "first prompt");
        assert_eq!(
            state.resolved.raw.codex_model.as_deref(),
            Some("profile-model")
        );
        assert_eq!(state.resolved.raw.linear_issue, vec!["ENG-123"]);
        assert!(state.resolved.raw.linear_until_complete);
        assert_eq!(state.resolved.raw.linear_limit, 7);
        assert_eq!(state.resolved.raw.linear_max_concurrency, 3);
        assert_eq!(
            state.resolved.raw.linear_workflow_file.as_deref(),
            Some(Path::new("docs/workflow.md"))
        );
        assert_eq!(state.resolved.raw.delivery_phases, vec!["plan", "verify"]);
        assert_eq!(state.resolved.raw.codex_capabilities, "override");
        assert!(state.profile_names.contains(&"roundtrip".to_string()));
    }

    #[test]
    fn provider_health_renders_onboarding_data() {
        let state = test_state("hello");

        let health_lines = provider_health_lines(&state);
        assert!(health_lines.iter().any(|line| {
            line.contains("codex bin:")
                && line.contains("auth:")
                && line.contains("model:gpt-5.2")
                && line.contains("effort:")
                && line.contains("cap:")
        }));

        let (rendered, _) = render_to_string(&state, 180, 46);
        assert!(rendered.contains("bin"));
        assert!(rendered.contains("cap"));
        assert!(rendered.contains("gpt-5.2"));
    }

    fn render_to_string(state: &StudioState, width: u16, height: u16) -> (String, bool) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render_studio(frame, state)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for row in buffer.content().chunks(width as usize) {
            for cell in row {
                rendered.push_str(cell.symbol());
            }
            rendered.push('\n');
        }
        let has_accent = buffer
            .content()
            .iter()
            .any(|cell| matches!(cell.fg, STUDIO_ACCENT | STUDIO_GOLD | STUDIO_PURPLE));
        (rendered, has_accent)
    }

    fn test_result(state: &StudioState) -> AmonHenResult {
        let codex = test_engine_result("codex", "planner", 1_000, 500, 1);
        let claude = test_engine_result("claude", "lead", 900, 400, 2);
        let gemini = test_engine_result("gemini", "executor", 700, 250, 0);
        let summary = test_engine_result("codex", "synthesis", 500, 200, 0);
        let members = vec![codex, claude, gemini];
        let workflow = build_workflow(&state.resolved);
        AmonHenResult {
            query: state.prompt.clone(),
            cwd: state.resolved.cwd.display().to_string(),
            members_requested: state.resolved.members.clone(),
            summarizer_requested: state.resolved.raw.summarizer.clone(),
            workflow: workflow.clone(),
            prompt_commands: vec![CommandTelemetry {
                command: "cargo test --workspace --locked".to_string(),
                status: "ok".to_string(),
                detail: String::new(),
                exit_code: Some(0),
                duration_ms: 1200,
                stdout_chars: 120,
                stderr_chars: 0,
                timed_out: false,
            }],
            iterations: vec![iteration_record(
                1,
                workflow.iterations,
                members.clone(),
                1200,
                None,
                None,
            )],
            members,
            summary,
        }
    }

    fn test_engine_result(
        name: &str,
        role: &str,
        input: usize,
        output: usize,
        tools: usize,
    ) -> EngineResult {
        EngineResult {
            name: name.to_string(),
            bin: Some(name.to_string()),
            status: "ok".to_string(),
            duration_ms: 1000,
            detail: String::new(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            output: "done".to_string(),
            command: format!("{name} run cargo test"),
            token_usage: TokenUsage {
                input,
                output,
                total: input + output,
                estimated: false,
                source: "test".to_string(),
            },
            tool_calls: (0..tools)
                .map(|index| ToolUsage {
                    name: format!("tool-{index}"),
                    kind: "command".to_string(),
                    status: "ok".to_string(),
                    detail: "cargo test".to_string(),
                })
                .collect(),
            sub_agents: Vec::new(),
            role: role.to_string(),
            iteration: 1,
            total_iterations: 1,
            team_size: 1,
        }
    }

    fn test_state(prompt: &str) -> StudioState {
        let args = CliArgs::try_parse_from([
            "amon-hen",
            "--studio",
            "--members",
            "codex,claude,gemini",
            "--planner",
            "codex",
            "--lead",
            "claude",
            "--handoff",
            "--iterations",
            "2",
            "--team-work",
            "1",
            "--codex-model",
            "gpt-5.2",
            "--claude-model",
            "sonnet",
            "--gemini-model",
            "gemini-pro",
            prompt,
        ])
        .unwrap();
        let resolved = resolve_args(args).unwrap();
        StudioState {
            prompt: resolved.prompt.clone(),
            resolved,
            menu_index: 0,
            focus: Pane::Capabilities,
            pane_order: PANES.to_vec(),
            setting_index: 0,
            capability_index: 0,
            linear_index: 0,
            result_index: 0,
            last_result: None,
            last_linear_result: None,
            last_auth_result: None,
            last_capability_result: None,
            run_job: None,
            run_events: Vec::new(),
            profile_name: "default".to_string(),
            profile_path: PathBuf::from(".amon-hen-studio-profiles.json"),
            profile_names: Vec::new(),
            provider_status: HashMap::new(),
            provider_detail: HashMap::new(),
            live_token_usage: HashMap::new(),
            live_tool_counts: HashMap::new(),
            live_sub_agents: HashMap::new(),
            status: "Ready".to_string(),
            input_mode: None,
            input_buffer: String::new(),
            show_help: false,
            exit_armed_until: None,
        }
    }
}

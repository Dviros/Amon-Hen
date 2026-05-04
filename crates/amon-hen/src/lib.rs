use clap::{error::ErrorKind, ArgAction, Parser, ValueEnum};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

mod linear_delivery;
mod studio;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_TIMEOUT_MS: u64 = 600_000;
const DEFAULT_MAX_MEMBER_CHARS: usize = 12_000;
const DEFAULT_ITERATIONS: usize = 1;
const DEFAULT_TEAM_SIZE: usize = 0;
const TOKEN_ESTIMATE_CHARS_PER_TOKEN: usize = 4;
const PROVIDER_STREAM_EVENT_MIN_INTERVAL: Duration = Duration::from_millis(250);

const ENGINES: [&str; 3] = ["codex", "claude", "gemini"];
const DEFAULT_SUMMARIZER_ORDER: [&str; 3] = ["codex", "claude", "gemini"];
const DEFAULT_AUTH_MODE: &str = "auto";
const CAPABILITY_INHERIT: &str = "inherit";
const CAPABILITY_OVERRIDE: &str = "override";
const PLANNER_MODE_BLOCKING: &str = "blocking";
const PLANNER_MODE_PARALLEL: &str = "parallel";
const PLANNER_MODE_REVIEW_CHAIN: &str = "review-chain";
const GEMINI_APPROVAL_MODES: [&str; 4] = ["plan", "default", "auto_edit", "yolo"];

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Engine {
    Codex,
    Claude,
    Gemini,
}

impl Engine {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "codex" => Some(Self::Codex),
            "claude" => Some(Self::Claude),
            "gemini" => Some(Self::Gemini),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
        }
    }

    fn binary_env_var(self) -> &'static str {
        match self {
            Self::Codex => "AMON_HEN_CODEX_BIN",
            Self::Claude => "AMON_HEN_CLAUDE_BIN",
            Self::Gemini => "AMON_HEN_GEMINI_BIN",
        }
    }

    fn allowed_efforts(self) -> &'static [&'static str] {
        match self {
            Self::Codex => &["low", "medium", "high", "xhigh", ""],
            Self::Claude => &["low", "medium", "high", "xhigh", "max"],
            Self::Gemini => &["low", "medium", "high", ""],
        }
    }
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "amon-hen",
    version,
    about = "Ask multiple AI CLIs the same question, then synthesize their answers.",
    trailing_var_arg = true,
    disable_version_flag = true
)]
pub struct CliArgs {
    #[arg(short = 'v', long = "version", action = ArgAction::SetTrue)]
    version: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,

    #[arg(long = "json-stream", alias = "ndjson", action = ArgAction::SetTrue)]
    json_stream: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    headless: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    studio: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    plain: bool,

    #[arg(long = "no-banner", action = ArgAction::SetTrue)]
    no_banner: bool,
    #[arg(long = "banner", hide = true, action = ArgAction::SetTrue)]
    banner: bool,

    #[arg(short = 'q', long = "quiet", alias = "summary-only", action = ArgAction::SetTrue)]
    summary_only: bool,

    #[arg(short = 'd', long, action = ArgAction::SetTrue)]
    verbose: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    all: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    codex: bool,
    #[arg(long = "no-codex", action = ArgAction::SetTrue)]
    no_codex: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    claude: bool,
    #[arg(long = "no-claude", action = ArgAction::SetTrue)]
    no_claude: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    gemini: bool,
    #[arg(long = "no-gemini", action = ArgAction::SetTrue)]
    no_gemini: bool,

    #[arg(long, value_delimiter = ',')]
    members: Vec<String>,

    #[arg(long, default_value = "auto")]
    summarizer: String,

    #[arg(long, value_enum)]
    effort: Option<Effort>,

    #[arg(long = "codex-model")]
    codex_model: Option<String>,
    #[arg(long = "claude-model")]
    claude_model: Option<String>,
    #[arg(long = "gemini-model")]
    gemini_model: Option<String>,

    #[arg(long = "codex-effort")]
    codex_effort: Option<String>,
    #[arg(long = "claude-effort")]
    claude_effort: Option<String>,
    #[arg(long = "gemini-effort")]
    gemini_effort: Option<String>,

    #[arg(long = "codex-sandbox", default_value = "read-only")]
    codex_sandbox: String,
    #[arg(long = "claude-permission-mode", default_value = "plan")]
    claude_permission_mode: String,
    #[arg(long = "gemini-approval-mode", default_value = "plan")]
    gemini_approval_mode: String,

    #[arg(long = "codex-auth", default_value = "auto")]
    codex_auth: String,
    #[arg(long = "claude-auth", default_value = "auto")]
    claude_auth: String,
    #[arg(long = "gemini-auth", default_value = "auto")]
    gemini_auth: String,

    #[arg(long = "codex-capabilities", default_value = "inherit")]
    codex_capabilities: String,
    #[arg(long = "claude-capabilities", default_value = "inherit")]
    claude_capabilities: String,
    #[arg(long = "gemini-capabilities", default_value = "inherit")]
    gemini_capabilities: String,

    #[arg(long = "codex-config", action = ArgAction::Append)]
    codex_config: Vec<String>,
    #[arg(long = "codex-mcp-profile")]
    codex_mcp_profile: Option<String>,
    #[arg(long = "claude-mcp-config", action = ArgAction::Append)]
    claude_mcp_config: Vec<String>,
    #[arg(long = "claude-allowed-tools", value_delimiter = ',', action = ArgAction::Append)]
    claude_allowed_tools: Vec<String>,
    #[arg(long = "claude-disallowed-tools", value_delimiter = ',', action = ArgAction::Append)]
    claude_disallowed_tools: Vec<String>,
    #[arg(long = "claude-tools", value_delimiter = ',', action = ArgAction::Append)]
    claude_tools: Vec<String>,
    #[arg(long = "claude-agent")]
    claude_agent: Option<String>,
    #[arg(long = "claude-agents-json")]
    claude_agents_json: Option<String>,
    #[arg(long = "claude-plugin-dir", action = ArgAction::Append)]
    claude_plugin_dir: Vec<String>,
    #[arg(long = "claude-strict-mcp-config", action = ArgAction::SetTrue)]
    claude_strict_mcp_config: bool,
    #[arg(long = "claude-disable-slash-commands", action = ArgAction::SetTrue)]
    claude_disable_slash_commands: bool,
    #[arg(long = "gemini-settings")]
    gemini_settings: Option<String>,
    #[arg(long = "gemini-tools-profile", value_delimiter = ',', action = ArgAction::Append)]
    gemini_tools_profile: Vec<String>,
    #[arg(long = "gemini-allowed-mcp-servers", value_delimiter = ',', action = ArgAction::Append)]
    gemini_allowed_mcp_servers: Vec<String>,
    #[arg(long = "gemini-policy", value_delimiter = ',', action = ArgAction::Append)]
    gemini_policy: Vec<String>,
    #[arg(long = "gemini-admin-policy", value_delimiter = ',', action = ArgAction::Append)]
    gemini_admin_policy: Vec<String>,
    #[arg(long = "capabilities-status", action = ArgAction::SetTrue)]
    capabilities_status: bool,

    #[arg(long = "auth-login", action = ArgAction::SetTrue)]
    auth_login: bool,
    #[arg(long = "auth-status", action = ArgAction::SetTrue)]
    auth_status: bool,
    #[arg(long = "auth-login-providers", value_delimiter = ',')]
    auth_login_providers: Vec<String>,
    #[arg(long = "auth-device-code", action = ArgAction::SetTrue)]
    auth_device_code: bool,
    #[arg(long = "no-auth-open-browser", action = ArgAction::SetTrue)]
    no_auth_open_browser: bool,
    #[arg(long = "auth-open-browser", hide = true, action = ArgAction::SetTrue)]
    auth_open_browser: bool,
    #[arg(long = "auth-timeout", default_value_t = 300)]
    auth_timeout: u64,
    #[arg(long = "claude-login-mode", default_value = "claudeai")]
    claude_login_mode: String,
    #[arg(long = "claude-login-email")]
    claude_login_email: Option<String>,

    #[arg(long = "file", alias = "tag-file", action = ArgAction::Append)]
    files: Vec<PathBuf>,
    #[arg(long = "cmd", alias = "prompt-command", action = ArgAction::Append)]
    commands: Vec<String>,

    #[arg(long, action = ArgAction::SetTrue)]
    handoff: bool,
    #[arg(long)]
    lead: Option<String>,
    #[arg(long)]
    planner: Option<String>,
    #[arg(long = "planner-mode", default_value = PLANNER_MODE_BLOCKING)]
    planner_mode: String,
    #[arg(long, default_value_t = DEFAULT_ITERATIONS)]
    iterations: usize,
    #[arg(long = "team-work", alias = "teamwork", alias = "sub-agents", default_value_t = DEFAULT_TEAM_SIZE)]
    team_work: usize,
    #[arg(long = "codex-sub-agents")]
    codex_sub_agents: Option<usize>,
    #[arg(long = "claude-sub-agents")]
    claude_sub_agents: Option<usize>,
    #[arg(long = "gemini-sub-agents")]
    gemini_sub_agents: Option<usize>,

    #[arg(long = "deliver-linear", alias = "linear", action = ArgAction::SetTrue)]
    deliver_linear: bool,
    #[arg(long = "linear-setup", action = ArgAction::SetTrue)]
    linear_setup: bool,
    #[arg(long = "linear-status", action = ArgAction::SetTrue)]
    linear_status: bool,
    #[arg(long = "linear-watch", action = ArgAction::SetTrue)]
    linear_watch: bool,
    #[arg(long = "linear-until-complete", action = ArgAction::SetTrue)]
    linear_until_complete: bool,
    #[arg(long = "linear-issue", value_delimiter = ',', action = ArgAction::Append)]
    linear_issue: Vec<String>,
    #[arg(long = "linear-query")]
    linear_query: Option<String>,
    #[arg(long = "linear-project", value_delimiter = ',', action = ArgAction::Append)]
    linear_project: Vec<String>,
    #[arg(long = "linear-epic", value_delimiter = ',', action = ArgAction::Append)]
    linear_epic: Vec<String>,
    #[arg(long = "linear-team")]
    linear_team: Option<String>,
    #[arg(long = "linear-state")]
    linear_state: Option<String>,
    #[arg(long = "linear-assignee")]
    linear_assignee: Option<String>,
    #[arg(long = "linear-limit", default_value_t = 3)]
    linear_limit: usize,
    #[arg(long = "linear-endpoint")]
    linear_endpoint: Option<String>,
    #[arg(long = "linear-auth", default_value = "api-key")]
    linear_auth: String,
    #[arg(long = "linear-api-key-env", default_value = "LINEAR_API_KEY")]
    linear_api_key_env: String,
    #[arg(long = "linear-oauth-token-env", default_value = "LINEAR_OAUTH_TOKEN")]
    linear_oauth_token_env: String,
    #[arg(long = "linear-completion-gate", default_value = "delivered")]
    linear_completion_gate: String,
    #[arg(long = "linear-review-state")]
    linear_review_state: Option<String>,
    #[arg(long = "linear-ci-timeout", default_value_t = 900)]
    linear_ci_timeout: u64,
    #[arg(long = "linear-ci-poll-interval", default_value_t = 30)]
    linear_ci_poll_interval: u64,
    #[arg(long = "linear-poll-interval", default_value_t = 60)]
    linear_poll_interval: u64,
    #[arg(long = "linear-max-polls")]
    linear_max_polls: Option<usize>,
    #[arg(long = "linear-max-concurrency", default_value_t = 1)]
    linear_max_concurrency: usize,
    #[arg(long = "linear-max-attempts", default_value_t = 3)]
    linear_max_attempts: usize,
    #[arg(long = "linear-retry-base", default_value_t = 60)]
    linear_retry_base: u64,
    #[arg(long = "linear-workspace-strategy", default_value = "worktree")]
    linear_workspace_strategy: String,
    #[arg(long = "linear-state-file")]
    linear_state_file: Option<PathBuf>,
    #[arg(long = "linear-workspace-root")]
    linear_workspace_root: Option<PathBuf>,
    #[arg(long = "linear-observability-dir")]
    linear_observability_dir: Option<PathBuf>,
    #[arg(long = "linear-workflow-file")]
    linear_workflow_file: Option<PathBuf>,
    #[arg(long = "linear-attach-media", value_delimiter = ',', action = ArgAction::Append)]
    linear_attach_media: Vec<String>,
    #[arg(long = "linear-attachment-title")]
    linear_attachment_title: Option<String>,
    #[arg(long = "no-linear-comments", action = ArgAction::SetTrue)]
    no_linear_comments: bool,
    #[arg(long = "linear-update-review-state", action = ArgAction::SetTrue)]
    linear_update_review_state: bool,
    #[arg(long = "delivery-phases", value_delimiter = ',')]
    delivery_phases: Vec<String>,

    #[arg(long, default_value_t = DEFAULT_TIMEOUT_MS / 1000)]
    timeout: u64,
    #[arg(long = "max-member-chars", default_value_t = DEFAULT_MAX_MEMBER_CHARS)]
    max_member_chars: usize,
    #[arg(long, default_value = ".")]
    cwd: PathBuf,
    #[arg(long, default_value = "auto")]
    color: String,
    #[arg(long = "no-color", action = ArgAction::SetTrue)]
    no_color: bool,

    #[arg(value_name = "QUERY")]
    prompt: Vec<String>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Effort {
    Low,
    Medium,
    High,
}

impl Effort {
    fn as_str(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedArgs {
    raw: CliArgs,
    members: Vec<String>,
    prompt: String,
    cwd: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmonHenResult {
    query: String,
    cwd: String,
    members_requested: Vec<String>,
    summarizer_requested: String,
    workflow: Workflow,
    prompt_commands: Vec<CommandTelemetry>,
    iterations: Vec<IterationRecord>,
    members: Vec<EngineResult>,
    summary: EngineResult,
}

#[derive(Debug, Clone, Serialize)]
struct Workflow {
    handoff: bool,
    lead: Option<String>,
    planner: Option<String>,
    planner_mode: String,
    iterations: usize,
    team_work: usize,
    teams: HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
struct IterationRecord {
    iteration: usize,
    total_iterations: usize,
    status: String,
    duration_ms: u128,
    members: Vec<EngineResult>,
    sub_agents: Vec<EngineResult>,
    handoff_context: Option<String>,
    summary_context: Option<String>,
    token_usage: TokenUsage,
    tool_calls: Vec<ToolUsage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineResult {
    name: String,
    bin: Option<String>,
    status: String,
    duration_ms: u128,
    detail: String,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    output: String,
    command: String,
    token_usage: TokenUsage,
    tool_calls: Vec<ToolUsage>,
    sub_agents: Vec<EngineResult>,
    role: String,
    iteration: usize,
    total_iterations: usize,
    team_size: usize,
}

#[derive(Debug, Clone, Serialize)]
struct TokenUsage {
    input: usize,
    output: usize,
    total: usize,
    estimated: bool,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
struct ToolUsage {
    name: String,
    kind: String,
    status: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct CommandTelemetry {
    command: String,
    status: String,
    detail: String,
    exit_code: Option<i32>,
    duration_ms: u128,
    stdout_chars: usize,
    stderr_chars: usize,
    timed_out: bool,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProgressStage {
    Context,
    Start,
    Spawn,
    Heartbeat,
    Done,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum RuntimeEventKind {
    Context,
    Iteration,
    Provider,
    TokenUsage,
    ToolUsage,
    Result,
}

#[derive(Debug, Clone, Serialize)]
struct ProgressEvent {
    kind: RuntimeEventKind,
    provider: Option<String>,
    role: Option<String>,
    stage: ProgressStage,
    status: Option<String>,
    iteration: Option<usize>,
    total_iterations: Option<usize>,
    is_sub_agent: bool,
    duration_ms: Option<u128>,
    token_usage: Option<TokenUsage>,
    tool_calls: Vec<ToolUsage>,
    message: String,
}

type ProgressSink = Arc<dyn Fn(ProgressEvent) + Send + Sync + 'static>;
type RuntimeEventSink = Arc<dyn Fn(RuntimeEvent) + Send + Sync + 'static>;

#[derive(Debug, Clone, Serialize)]
struct RuntimeEvent {
    sequence: u64,
    elapsed_ms: u128,
    #[serde(flatten)]
    progress: ProgressEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Box<AmonHenResult>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "linearResult")]
    linear_result: Option<Box<linear_delivery::LinearDeliveryResult>>,
}

#[derive(Clone)]
struct RuntimeEventBus {
    started: Instant,
    sequence: Arc<AtomicU64>,
    emit_lock: Arc<Mutex<()>>,
    sinks: Arc<Mutex<Vec<RuntimeEventSink>>>,
}

#[derive(Debug)]
struct PromptContext {
    prompt: String,
    commands: Vec<CommandTelemetry>,
}

#[derive(Debug)]
struct CommandResult {
    command: String,
    args: Vec<String>,
    code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
    cancelled: bool,
    error: Option<String>,
    timeout_ms: u64,
    duration_ms: u128,
}

#[derive(Clone)]
struct CommandProgress {
    label: String,
    sink: Option<ProgressSink>,
    input_tokens: usize,
}

struct CommandRequest<'a> {
    command: &'a str,
    args: &'a [String],
    cwd: &'a Path,
    stdin_text: Option<&'a str>,
    timeout_ms: u64,
    envs: HashMap<String, String>,
    progress: Option<CommandProgress>,
    cancel: Option<Arc<AtomicBool>>,
}

impl<'a> CommandRequest<'a> {
    fn new(command: &'a str, args: &'a [String], cwd: &'a Path, timeout_ms: u64) -> Self {
        Self {
            command,
            args,
            cwd,
            stdin_text: None,
            timeout_ms,
            envs: HashMap::new(),
            progress: None,
            cancel: None,
        }
    }

    fn stdin_text(mut self, stdin_text: Option<&'a str>) -> Self {
        self.stdin_text = stdin_text;
        self
    }

    fn envs(mut self, envs: HashMap<String, String>) -> Self {
        self.envs = envs;
        self
    }

    fn progress(mut self, progress: Option<CommandProgress>) -> Self {
        self.progress = progress;
        self
    }

    fn cancel(mut self, cancel: Option<Arc<AtomicBool>>) -> Self {
        self.cancel = cancel;
        self
    }
}

#[derive(Clone)]
struct EngineRunOptions {
    prompt: String,
    cwd: PathBuf,
    timeout_ms: u64,
    effort: Option<String>,
    model: Option<String>,
    permission: Option<String>,
    auth: String,
    capability: ProviderCapability,
    role: String,
    iteration: usize,
    total_iterations: usize,
    team_size: usize,
    is_sub_agent: bool,
    live: bool,
    progress: Option<ProgressSink>,
    cancel: Option<Arc<AtomicBool>>,
}

struct EngineOptionsInput<'a> {
    member: &'a str,
    prompt: String,
    role: &'a str,
    iteration: usize,
    workflow: &'a Workflow,
    progress: Option<ProgressSink>,
    cancel: Option<Arc<AtomicBool>>,
}

struct MemberPromptInput<'a> {
    query: &'a str,
    role: &'a str,
    workflow: &'a Workflow,
    iteration: usize,
    team_size: usize,
    previous_iteration: &'a [EngineResult],
    handoff_results: &'a [EngineResult],
    plan_output: &'a str,
}

#[derive(Debug, Clone)]
struct ProviderCapability {
    mode: String,
    config: Vec<String>,
    mcp_profile: Option<String>,
    mcp_config: Vec<String>,
    allowed_tools: Vec<String>,
    disallowed_tools: Vec<String>,
    tools: Vec<String>,
    agent: Option<String>,
    agents_json: Option<String>,
    plugin_dirs: Vec<String>,
    strict_mcp_config: bool,
    disable_slash_commands: bool,
    settings: Option<String>,
    tools_profile: Vec<String>,
    allowed_mcp_servers: Vec<String>,
    policy: Vec<String>,
    admin_policy: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProviderAuthStatus {
    provider: String,
    configured: bool,
    status: String,
    source: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProviderCapabilityStatus {
    provider: String,
    mcp: CommandTelemetry,
    skills: Option<CommandTelemetry>,
    tools: Option<CommandTelemetry>,
    detail: String,
}

struct TempSettings {
    _dir: tempfile::TempDir,
    path: PathBuf,
}

pub fn run_from_env() -> i32 {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    run_with_args(args)
}

pub fn run_with_args<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let raw_args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if raw_args.first().and_then(|value| value.to_str()) == Some("help") {
        println!("{}", render_cli_help());
        return 0;
    }

    let mut parse_args = vec![OsString::from("amon-hen")];
    parse_args.extend(raw_args);
    let parsed = match CliArgs::try_parse_from(parse_args) {
        Ok(parsed) => parsed,
        Err(error) => {
            if error.kind() == ErrorKind::DisplayHelp {
                println!("{}", render_cli_help());
                return 0;
            }
            if error.kind() == ErrorKind::DisplayVersion {
                println!("{VERSION}");
                return 0;
            }
            let exit_code = parse_error_exit_code(error.kind());
            let _ = error.print();
            return exit_code;
        }
    };

    if parsed.version {
        println!("{VERSION}");
        return 0;
    }

    let resolved = match resolve_args(parsed) {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("{error}");
            return 64;
        }
    };

    if resolved.raw.studio {
        return studio::run_studio(&resolved);
    }

    let event_bus = resolved.raw.json_stream.then(RuntimeEventBus::new);
    if let Some(bus) = &event_bus {
        let stdout_lock = Arc::new(Mutex::new(()));
        bus.add_sink(Arc::new(move |event| {
            let Ok(line) = serde_json::to_string(&event) else {
                return;
            };
            let _guard = stdout_lock.lock().ok();
            println!("{line}");
            let _ = io::stdout().flush();
        }));
    }
    let progress = event_bus.as_ref().map(RuntimeEventBus::progress_sink);

    if resolved.raw.auth_status {
        println!(
            "{}",
            render_auth_statuses(&collect_auth_statuses(&resolved))
        );
        if resolved.prompt.trim().is_empty()
            && !resolved.raw.auth_login
            && !resolved.raw.deliver_linear
            && !resolved.raw.capabilities_status
        {
            return 0;
        }
    }

    if resolved.raw.capabilities_status {
        println!(
            "{}",
            render_provider_capability_statuses(&collect_provider_capability_statuses(&resolved))
        );
        if resolved.prompt.trim().is_empty()
            && !resolved.raw.auth_login
            && !resolved.raw.deliver_linear
        {
            return 0;
        }
    }

    if resolved.raw.auth_login {
        if let Err(error) = run_social_login(&resolved) {
            eprintln!("{error}");
            return 1;
        }
        if resolved.prompt.trim().is_empty() && !resolved.raw.deliver_linear {
            return 0;
        }
    }

    if resolved.raw.linear_setup || resolved.raw.linear_status {
        match linear_delivery::get_linear_status(&resolved) {
            Ok(status) => {
                println!("{}", linear_delivery::render_linear_status(&status));
                return 0;
            }
            Err(error) => {
                eprintln!("{error}");
                return 1;
            }
        }
    }

    if linear_delivery_requested(&resolved.raw) {
        match linear_delivery::run_linear_delivery_with_progress(&resolved, progress.clone(), None)
        {
            Ok(result) => {
                if let Some(bus) = &event_bus {
                    bus.emit_linear_result(&result);
                } else if resolved.raw.json {
                    let serialized = serde_json::to_string_pretty(&result);
                    println!("{}", serialized.unwrap_or_else(|_| "{}".to_string()));
                } else {
                    println!(
                        "{}",
                        linear_delivery::render_linear_delivery_result(&result)
                    );
                }
                return if result.success { 0 } else { 1 };
            }
            Err(error) => {
                eprintln!("{error}");
                return 1;
            }
        }
    }

    if resolved.prompt.trim().is_empty() {
        eprintln!("No query provided.\n\n{}", render_cli_help());
        return 64;
    }

    let prompt_context = match build_prompt_context_with_progress(&resolved, progress.clone()) {
        Ok(context) => context,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };

    if should_show_banner(&resolved.raw) {
        eprintln!("{}", render_banner());
    }

    let result = run_amon_hen_with_progress(
        &resolved,
        prompt_context.prompt,
        prompt_context.commands,
        progress,
    );
    if let Some(bus) = &event_bus {
        bus.emit_result(&result);
    } else if resolved.raw.json {
        let serialized = serde_json::to_string_pretty(&result);
        println!("{}", serialized.unwrap_or_else(|_| "{}".to_string()));
    } else {
        println!("{}", render_human_result(&result, resolved.raw.verbose));
    }

    if is_success(&result) {
        0
    } else {
        1
    }
}

fn parse_error_exit_code(kind: ErrorKind) -> i32 {
    match kind {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => 0,
        _ => 64,
    }
}

fn render_cli_help() -> &'static str {
    r#"Amon Hen
Rust-native orchestration for Codex, Claude, Gemini, and Linear delivery.

Usage:
  amon-hen [OPTIONS] "your task"
  amon-hen --studio --members codex,claude,gemini "your task"
  amon-hen --deliver-linear --linear-project "Project" --linear-until-complete

Core run:
  --members codex,claude,gemini       Providers to consult. Also supports --codex, --claude, --gemini, --all.
  --summarizer auto|codex|claude|gemini
                                      Provider used for synthesis. auto tries the configured order.
  --studio                            Open the interactive native Studio.
  --plain                             Disable rich terminal output for script-friendly runs.
  --json, --json-stream               Emit JSON or newline-delimited JSON.
  -q, --quiet                         Print the synthesis only.
  -d, --verbose                       Include provider details, telemetry, and tool usage.

Models, effort, and permissions:
  --codex-model NAME                  Model passed to Codex.
  --claude-model NAME                 Model passed to Claude.
  --gemini-model NAME                 Model passed to Gemini.
  --effort low|medium|high            Shared default effort.
  --codex-effort low|medium|high|xhigh
  --claude-effort low|medium|high|xhigh|max
  --gemini-effort low|medium|high
  --codex-sandbox read-only|workspace-write|danger-full-access
  --claude-permission-mode MODE       Claude permission mode such as plan, acceptEdits, bypassPermissions.
  --gemini-approval-mode MODE         Gemini approval mode: plan, default, auto_edit, or yolo.

Auth and provider capabilities:
  --auth-login                        Start provider social-login flows.
  --auth-status                       Show configured auth sources for each provider.
  --codex-auth auto|api-key|social-login|oauth|keychain
  --claude-auth auto|api-key|social-login|oauth|keychain
  --gemini-auth auto|api-key|social-login|oauth|keychain
  --capabilities-status               Probe provider Skills / MCP / Tools support.
  --codex-capabilities inherit|override
  --claude-capabilities inherit|override
  --gemini-capabilities inherit|override
  --codex-config PATH                 Extra Codex config file, repeatable.
  --codex-mcp-profile NAME            Codex MCP profile.
  --claude-mcp-config PATH            Claude MCP config file, repeatable.
  --claude-allowed-tools LIST         Allowed Claude tools.
  --claude-disallowed-tools LIST      Disallowed Claude tools.
  --claude-tools LIST                 Claude tool list override.
  --claude-agent NAME                 Claude agent profile.
  --claude-agents-json JSON           Claude agents JSON override.
  --claude-plugin-dir PATH            Claude plugin directory, repeatable.
  --gemini-settings PATH              Gemini settings file.
  --gemini-tools-profile LIST         Gemini tool profile list.
  --gemini-allowed-mcp-servers LIST   Gemini MCP allow-list.
  --gemini-policy LIST                Gemini policy values.
  --gemini-admin-policy LIST          Gemini admin policy values.

Team workflow:
  --handoff                           Feed planner/lead context between providers.
  --planner codex|claude|gemini       Assign the planning role.
  --planner-mode blocking|parallel|review-chain
                                      Wait, run beside executors, or serially review each agent.
  --lead codex|claude|gemini          Assign the lead reviewer/synthesizer role.
  --iterations N                      Run multiple provider rounds per prompt.
  --team-work N                       Spawn N same-provider sub-agents per provider.
  --codex-sub-agents N                Override Codex sub-agent count.
  --claude-sub-agents N               Override Claude sub-agent count.
  --gemini-sub-agents N               Override Gemini sub-agent count.
  --file PATH                         Tag a local file into the prompt context, repeatable.
  --cmd COMMAND                       Run a local command and include its telemetry, repeatable.

Linear delivery:
  --deliver-linear                    Deliver Linear work instead of a one-shot prompt.
  --linear-watch                      Poll Linear for matching work.
  --linear-until-complete             Continue until review/delivery gate is met.
  --linear-issue ID,ID                Target specific Linear issues.
  --linear-project NAME,NAME          Target Linear projects.
  --linear-epic NAME,NAME             Target Linear epics.
  --linear-team KEY                   Restrict to a Linear team.
  --linear-state NAME                 Restrict by Linear workflow state.
  --linear-completion-gate delivered|review|ci
  --linear-max-concurrency N          Isolated workspace concurrency.
  --linear-max-attempts N             Retry attempts per issue.
  --linear-workspace-strategy worktree|local
  --linear-workspace-root PATH        Root for per-issue workspaces.
  --linear-observability-dir PATH     Logs, reconciliation, and run telemetry.
  --linear-attach-media PATH,PATH     Attach generated media back to Linear.

Runtime:
  --cwd PATH                          Working directory for provider CLIs.
  --timeout SECONDS                   Per-provider timeout.
  --max-member-chars N                Max provider output included in synthesis.
  --color auto|always|never           Color mode.
  --no-color                          Disable color.
  -v, --version                       Print version.
  -h, --help                          Show this help.

Examples:
  amon-hen --studio --members codex,claude,gemini "Inspect this repo"
  amon-hen --members codex,claude,gemini --planner codex --lead claude --handoff --iterations 2 "Suggest the cleanest next patch"
  amon-hen --deliver-linear --linear-project "Developer Platform" --linear-until-complete --team-work 2
  amon-hen --auth-login --auth-login-providers codex,claude,gemini"#
}

fn resolve_args(raw: CliArgs) -> Result<ResolvedArgs, String> {
    let members = resolve_members(&raw)?;
    validate_engine_name(&raw.summarizer, true, "--summarizer")?;
    if let Some(lead) = &raw.lead {
        validate_engine_name(lead, false, "--lead")?;
        if !members.contains(lead) {
            return Err(format!(
                "--lead must be one of the enabled members: {}",
                members.join(", ")
            ));
        }
    }
    if let Some(planner) = &raw.planner {
        validate_engine_name(planner, false, "--planner")?;
        if !members.contains(planner) {
            return Err(format!(
                "--planner must be one of the enabled members: {}",
                members.join(", ")
            ));
        }
    }
    validate_choice(
        "--planner-mode",
        &raw.planner_mode,
        &[
            PLANNER_MODE_BLOCKING,
            PLANNER_MODE_PARALLEL,
            PLANNER_MODE_REVIEW_CHAIN,
        ],
    )?;
    validate_provider_effort("codex", raw.codex_effort.as_deref())?;
    validate_provider_effort("claude", raw.claude_effort.as_deref())?;
    validate_provider_effort("gemini", raw.gemini_effort.as_deref())?;
    validate_choice(
        "--gemini-approval-mode",
        &raw.gemini_approval_mode,
        &GEMINI_APPROVAL_MODES,
    )?;
    validate_choice(
        "--claude-login-mode",
        &raw.claude_login_mode,
        &["claudeai", "console", "sso"],
    )?;
    validate_choice(
        "--codex-capabilities",
        &raw.codex_capabilities,
        &[CAPABILITY_INHERIT, CAPABILITY_OVERRIDE],
    )?;
    validate_choice(
        "--claude-capabilities",
        &raw.claude_capabilities,
        &[CAPABILITY_INHERIT, CAPABILITY_OVERRIDE],
    )?;
    validate_choice(
        "--gemini-capabilities",
        &raw.gemini_capabilities,
        &[CAPABILITY_INHERIT, CAPABILITY_OVERRIDE],
    )?;
    validate_choice("--linear-auth", &raw.linear_auth, &["api-key", "oauth"])?;
    validate_choice(
        "--linear-completion-gate",
        &raw.linear_completion_gate,
        &["delivered", "human-review", "ci-success", "review-or-ci"],
    )?;
    validate_choice(
        "--linear-workspace-strategy",
        &raw.linear_workspace_strategy,
        &["worktree", "copy", "none"],
    )?;
    if raw.linear_limit == 0 {
        return Err("--linear-limit requires a positive integer.".to_string());
    }
    if raw.linear_max_concurrency == 0 {
        return Err("--linear-max-concurrency requires a positive integer.".to_string());
    }
    if raw.linear_max_polls == Some(0) {
        return Err("--linear-max-polls requires a positive integer.".to_string());
    }
    if raw
        .linear_endpoint
        .as_ref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err("--linear-endpoint requires a non-empty value.".to_string());
    }
    for phase in raw
        .delivery_phases
        .iter()
        .map(|phase| phase.trim())
        .filter(|phase| !phase.is_empty())
    {
        validate_choice(
            "--delivery-phases",
            phase,
            &["plan", "implement", "verify", "ship"],
        )?;
    }

    let mut prompt = raw.prompt.join(" ");
    if prompt.trim().is_empty() && !io::stdin().is_terminal() {
        let mut stdin = String::new();
        io::stdin()
            .read_to_string(&mut stdin)
            .map_err(|error| format!("Failed to read stdin: {error}"))?;
        prompt = stdin;
    }
    let cwd = raw.cwd.canonicalize().unwrap_or_else(|_| raw.cwd.clone());

    Ok(ResolvedArgs {
        raw,
        members,
        prompt,
        cwd,
    })
}

fn resolve_members(raw: &CliArgs) -> Result<Vec<String>, String> {
    let mut members = if raw.members.is_empty() {
        ENGINES
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>()
    } else {
        raw.members
            .iter()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>()
    };

    if raw.all {
        members = ENGINES.iter().map(|name| (*name).to_string()).collect();
    }
    if raw.no_codex {
        members.retain(|name| name != "codex");
    }
    if raw.no_claude {
        members.retain(|name| name != "claude");
    }
    if raw.no_gemini {
        members.retain(|name| name != "gemini");
    }
    for enabled in [
        (raw.codex, "codex"),
        (raw.claude, "claude"),
        (raw.gemini, "gemini"),
    ] {
        if enabled.0 && !members.iter().any(|name| name == enabled.1) {
            members.push(enabled.1.to_string());
        }
    }

    let mut seen = HashSet::new();
    members.retain(|member| seen.insert(member.clone()));
    for member in &members {
        validate_engine_name(member, false, "--members")?;
    }
    if members.is_empty() {
        return Err("At least one member must be enabled.".to_string());
    }
    Ok(members)
}

fn validate_engine_name(name: &str, allow_auto: bool, flag: &str) -> Result<(), String> {
    if allow_auto && name == "auto" {
        return Ok(());
    }
    if Engine::parse(name).is_some() {
        Ok(())
    } else {
        Err(format!(
            "{flag} must be one of: {}",
            if allow_auto {
                "auto, codex, claude, gemini"
            } else {
                "codex, claude, gemini"
            }
        ))
    }
}

fn validate_provider_effort(provider: &str, value: Option<&str>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let engine = Engine::parse(provider).expect("provider effort validation uses known engines");
    if engine.allowed_efforts().contains(&value) {
        Ok(())
    } else {
        Err(format!("Unsupported --{provider}-effort value: {value}"))
    }
}

fn validate_choice(flag: &str, value: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!("{flag} must be one of: {}", allowed.join(", ")))
    }
}

fn collect_auth_statuses(resolved: &ResolvedArgs) -> Vec<ProviderAuthStatus> {
    collect_auth_statuses_with_cancel(resolved, None)
}

fn collect_auth_statuses_with_cancel(
    resolved: &ResolvedArgs,
    cancel: Option<Arc<AtomicBool>>,
) -> Vec<ProviderAuthStatus> {
    resolved
        .members
        .iter()
        .map(|provider| provider_auth_status_with_cancel(provider, resolved, cancel.clone()))
        .collect()
}

fn provider_auth_status(provider: &str, resolved: &ResolvedArgs) -> ProviderAuthStatus {
    provider_auth_status_with_cancel(provider, resolved, None)
}

fn provider_auth_status_with_cancel(
    provider: &str,
    resolved: &ResolvedArgs,
    cancel: Option<Arc<AtomicBool>>,
) -> ProviderAuthStatus {
    let auth = provider_auth(resolved, provider);
    if cancel
        .as_ref()
        .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
    {
        return ProviderAuthStatus {
            provider: provider.to_string(),
            configured: false,
            status: "cancelled".to_string(),
            source: auth,
            detail: "Auth status refresh cancelled.".to_string(),
        };
    }
    match provider {
        "codex" => {
            let bin = resolve_binary("codex");
            let args = vec!["login".to_string(), "status".to_string()];
            status_from_command(provider, &auth, &bin, &args, &resolved.cwd, cancel)
        }
        "claude" => {
            let bin = resolve_binary("claude");
            let args = vec![
                "auth".to_string(),
                "status".to_string(),
                "--text".to_string(),
            ];
            status_from_command(provider, &auth, &bin, &args, &resolved.cwd, cancel)
        }
        "gemini" => {
            let has_api_key = std::env::var("GEMINI_API_KEY")
                .ok()
                .is_some_and(|value| !value.trim().is_empty());
            let home = std::env::var("HOME").unwrap_or_default();
            let has_oauth_file = !home.is_empty()
                && [
                    ".gemini/oauth_creds.json",
                    ".gemini/oauth_tokens.json",
                    ".gemini/settings.json",
                ]
                .iter()
                .any(|path| Path::new(&home).join(path).exists());
            let configured = has_api_key || has_oauth_file;
            ProviderAuthStatus {
                provider: provider.to_string(),
                configured,
                status: if configured { "configured" } else { "unknown" }.to_string(),
                source: auth,
                detail: if has_api_key {
                    "GEMINI_API_KEY is present.".to_string()
                } else if has_oauth_file {
                    "Gemini local auth/config files are present.".to_string()
                } else {
                    "Gemini CLI does not currently expose a stable headless auth status command; use Social login from Studio or run gemini interactively.".to_string()
                },
            }
        }
        _ => ProviderAuthStatus {
            provider: provider.to_string(),
            configured: false,
            status: "unknown".to_string(),
            source: auth,
            detail: "Unknown provider.".to_string(),
        },
    }
}

fn status_from_command(
    provider: &str,
    auth: &str,
    bin: &str,
    args: &[String],
    cwd: &Path,
    cancel: Option<Arc<AtomicBool>>,
) -> ProviderAuthStatus {
    let result = run_command(CommandRequest::new(bin, args, cwd, 15_000).cancel(cancel));
    let configured = result.code == Some(0);
    ProviderAuthStatus {
        provider: provider.to_string(),
        configured,
        status: if configured {
            "configured"
        } else if result.cancelled {
            "cancelled"
        } else if result.error.is_some() {
            "missing-cli"
        } else {
            "not-configured"
        }
        .to_string(),
        source: auth.to_string(),
        detail: sanitize_status_detail(&compact_failure(&result)),
    }
}

fn sanitize_status_detail(detail: &str) -> String {
    let cleaned = strip_terminal_control_sequences(detail);
    let detail = cleaned.trim();
    if detail.is_empty() {
        return "No status detail returned.".to_string();
    }
    let detail = detail
        .lines()
        .take(6)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(redact_possible_secret)
        .collect::<Vec<_>>()
        .join(" ");
    redact_local_paths(&detail)
}

fn strip_terminal_control_sequences(value: &str) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '\x1b' {
            index = skip_terminal_escape(&chars, index + 1);
            continue;
        }
        if chars[index] == '\u{9b}' {
            index = skip_csi_sequence(&chars, index + 1);
            continue;
        }
        if let Some(after_escape) = escaped_escape_end(&chars, index) {
            index = skip_terminal_escape(&chars, after_escape);
            continue;
        }
        let ch = chars[index];
        if ch.is_control() && !matches!(ch, '\n' | '\r' | '\t') {
            output.push(' ');
        } else {
            output.push(ch);
        }
        index += 1;
    }
    output
}

fn escaped_escape_end(chars: &[char], index: usize) -> Option<usize> {
    if chars.get(index) != Some(&'\\') {
        return None;
    }
    let mut cursor = index;
    while chars.get(cursor) == Some(&'\\') {
        cursor += 1;
    }
    if starts_with_chars(chars, cursor, &['u', '0', '0', '1', 'b'])
        || starts_with_chars(chars, cursor, &['u', '0', '0', '1', 'B'])
    {
        return Some(cursor + 5);
    }
    if starts_with_chars(chars, cursor, &['u', '{', '1', 'b', '}'])
        || starts_with_chars(chars, cursor, &['u', '{', '1', 'B', '}'])
    {
        return Some(cursor + 5);
    }
    if starts_with_chars(chars, cursor, &['x', '1', 'b'])
        || starts_with_chars(chars, cursor, &['x', '1', 'B'])
    {
        return Some(cursor + 3);
    }
    if starts_with_chars(chars, cursor, &['0', '3', '3']) {
        return Some(cursor + 3);
    }
    if starts_with_chars(chars, cursor, &['e']) {
        return Some(cursor + 1);
    }
    None
}

fn starts_with_chars(chars: &[char], index: usize, needle: &[char]) -> bool {
    chars
        .get(index..index.saturating_add(needle.len()))
        .is_some_and(|slice| slice == needle)
}

fn skip_terminal_escape(chars: &[char], index: usize) -> usize {
    let Some(ch) = chars.get(index).copied() else {
        return index;
    };
    match ch {
        '[' => skip_csi_sequence(chars, index + 1),
        ']' => skip_terminated_control_string(chars, index + 1),
        'P' | '^' | '_' | 'X' => skip_terminated_control_string(chars, index + 1),
        '(' | ')' | '*' | '+' | '-' | '.' | '/' => (index + 2).min(chars.len()),
        '@'..='_' => (index + 1).min(chars.len()),
        _ => index,
    }
}

fn skip_csi_sequence(chars: &[char], mut index: usize) -> usize {
    while index < chars.len() {
        let ch = chars[index];
        index += 1;
        if ('@'..='~').contains(&ch) {
            break;
        }
    }
    index
}

fn skip_terminated_control_string(chars: &[char], mut index: usize) -> usize {
    while index < chars.len() {
        if chars[index] == '\u{7}' {
            return index + 1;
        }
        if chars[index] == '\x1b' && chars.get(index + 1) == Some(&'\\') {
            return index + 2;
        }
        if let Some(after_escape) = escaped_escape_end(chars, index) {
            if chars.get(after_escape) == Some(&'\\') {
                return after_escape + 1;
            }
            index = after_escape;
            continue;
        }
        index += 1;
    }
    index
}

fn redact_possible_secret(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| {
            if looks_sensitive_token(token) {
                "[redacted]"
            } else if looks_like_email(token) {
                "[email]"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_sensitive_token(token: &str) -> bool {
    let trimmed =
        token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_');
    trimmed.len() >= 28
        && (trimmed.starts_with("sk-")
            || trimmed.starts_with("sk_")
            || trimmed.starts_with("gho_")
            || trimmed.starts_with("ghp_")
            || trimmed.starts_with("ghs_")
            || trimmed.starts_with("ghu_")
            || trimmed.starts_with("AIza")
            || trimmed.starts_with("AQ."))
}

fn looks_like_email(token: &str) -> bool {
    let trimmed = token.trim_matches(|ch: char| {
        !ch.is_ascii_alphanumeric() && !matches!(ch, '@' | '.' | '_' | '-' | '+')
    });
    trimmed.contains('@') && trimmed.rsplit_once('.').is_some()
}

fn redact_local_paths(value: &str) -> String {
    let mut redacted = value.to_string();
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            redacted = redacted.replace(&home, "~");
        }
    }
    redacted
}

fn render_auth_statuses(statuses: &[ProviderAuthStatus]) -> String {
    let mut lines = vec!["Provider auth status".to_string()];
    for status in statuses {
        lines.push(format!(
            "- {}: {} via {} ({})",
            status.provider, status.status, status.source, status.detail
        ));
    }
    lines.join("\n")
}

fn collect_provider_capability_statuses(resolved: &ResolvedArgs) -> Vec<ProviderCapabilityStatus> {
    collect_provider_capability_statuses_with_cancel(resolved, None)
}

fn collect_provider_capability_statuses_with_cancel(
    resolved: &ResolvedArgs,
    cancel: Option<Arc<AtomicBool>>,
) -> Vec<ProviderCapabilityStatus> {
    resolved
        .members
        .iter()
        .map(|provider| provider_capability_status(provider, resolved, cancel.clone()))
        .collect()
}

fn provider_capability_status(
    provider: &str,
    resolved: &ResolvedArgs,
    cancel: Option<Arc<AtomicBool>>,
) -> ProviderCapabilityStatus {
    match provider {
        "codex" => {
            let bin = resolve_binary("codex");
            let mcp = run_capability_probe(&bin, &["mcp", "list"], &resolved.cwd, cancel.clone());
            ProviderCapabilityStatus {
                provider: provider.to_string(),
                mcp,
                skills: None,
                tools: Some(run_capability_probe(
                    &bin,
                    &["plugin", "marketplace", "--help"],
                    &resolved.cwd,
                    cancel,
                )),
                detail:
                    "Codex inherits ~/.codex config unless --codex-capabilities override is set."
                        .to_string(),
            }
        }
        "claude" => {
            let bin = resolve_binary("claude");
            ProviderCapabilityStatus {
                provider: provider.to_string(),
                mcp: run_capability_probe(&bin, &["mcp", "list"], &resolved.cwd, cancel.clone()),
                skills: Some(run_capability_probe(
                    &bin,
                    &["plugin", "list"],
                    &resolved.cwd,
                    cancel.clone(),
                )),
                tools: Some(run_capability_probe(
                    &bin,
                    &["agents"],
                    &resolved.cwd,
                    cancel,
                )),
                detail: "Claude override can manage MCP config, tools, agents, plugin dirs, and slash-command skills.".to_string(),
            }
        }
        "gemini" => {
            let bin = resolve_binary("gemini");
            ProviderCapabilityStatus {
                provider: provider.to_string(),
                mcp: run_capability_probe(&bin, &["mcp", "list"], &resolved.cwd, cancel.clone()),
                skills: Some(run_capability_probe(
                    &bin,
                    &["skills", "list"],
                    &resolved.cwd,
                    cancel.clone(),
                )),
                tools: Some(run_capability_probe(
                    &bin,
                    &["extensions", "list"],
                    &resolved.cwd,
                    cancel,
                )),
                detail: "Gemini override can manage settings, extensions, MCP server allowlists, and policy files.".to_string(),
            }
        }
        _ => ProviderCapabilityStatus {
            provider: provider.to_string(),
            mcp: CommandTelemetry {
                command: provider.to_string(),
                status: "unknown".to_string(),
                detail: "Unknown provider.".to_string(),
                exit_code: None,
                duration_ms: 0,
                stdout_chars: 0,
                stderr_chars: 0,
                timed_out: false,
            },
            skills: None,
            tools: None,
            detail: "Unknown provider.".to_string(),
        },
    }
}

fn run_capability_probe(
    bin: &str,
    args: &[&str],
    cwd: &Path,
    cancel: Option<Arc<AtomicBool>>,
) -> CommandTelemetry {
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    command_telemetry(&run_command(
        CommandRequest::new(bin, &args, cwd, 20_000).cancel(cancel),
    ))
}

fn emit_runtime_event(
    progress: &Option<ProgressSink>,
    fallback_to_stderr: bool,
    event: ProgressEvent,
) {
    if let Some(progress) = progress {
        progress(event);
    } else if fallback_to_stderr {
        eprintln!("{}", event.message);
    }
}

impl RuntimeEventBus {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            sequence: Arc::new(AtomicU64::new(1)),
            emit_lock: Arc::new(Mutex::new(())),
            sinks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn add_sink(&self, sink: RuntimeEventSink) {
        if let Ok(mut sinks) = self.sinks.lock() {
            sinks.push(sink);
        }
    }

    fn progress_sink(&self) -> ProgressSink {
        let bus = self.clone();
        Arc::new(move |event| bus.emit(event, None, None))
    }

    fn emit_result(&self, result: &AmonHenResult) {
        let status = if is_success(result) { "ok" } else { "error" };
        self.emit(
            ProgressEvent {
                kind: RuntimeEventKind::Result,
                provider: Some(result.summary.name.clone()),
                role: Some(result.summary.role.clone()),
                stage: ProgressStage::Done,
                status: Some(status.to_string()),
                iteration: Some(result.workflow.iterations),
                total_iterations: Some(result.workflow.iterations),
                is_sub_agent: false,
                duration_ms: Some(result.summary.duration_ms),
                token_usage: Some(result.summary.token_usage.clone()),
                tool_calls: result.summary.tool_calls.clone(),
                message: format!("[amon-hen] result {status}"),
            },
            Some(Box::new(result.clone())),
            None,
        );
    }

    fn emit_linear_result(&self, result: &linear_delivery::LinearDeliveryResult) {
        let status = if result.success { "ok" } else { "error" };
        self.emit(
            progress_event_with_context(
                Some("linear"),
                Some("delivery"),
                ProgressStage::Done,
                Some(status),
                None,
                None,
                false,
                Some(result.duration_ms),
                None,
                vec![],
                format!("[amon-hen] linear delivery {status}"),
            ),
            None,
            Some(Box::new(result.clone())),
        );
    }

    fn emit(
        &self,
        progress: ProgressEvent,
        result: Option<Box<AmonHenResult>>,
        linear_result: Option<Box<linear_delivery::LinearDeliveryResult>>,
    ) {
        let _emit_guard = self.emit_lock.lock().ok();
        let event = RuntimeEvent {
            sequence: self.sequence.fetch_add(1, Ordering::SeqCst),
            elapsed_ms: self.started.elapsed().as_millis(),
            progress,
            result,
            linear_result,
        };
        let sinks = self
            .sinks
            .lock()
            .map(|sinks| sinks.clone())
            .unwrap_or_default();
        for sink in sinks {
            sink(event.clone());
        }
    }
}

fn progress_event(
    provider: Option<&str>,
    role: Option<&str>,
    stage: ProgressStage,
    status: Option<&str>,
    message: impl Into<String>,
) -> ProgressEvent {
    progress_event_with_context(
        provider,
        role,
        stage,
        status,
        None,
        None,
        false,
        None,
        None,
        vec![],
        message,
    )
}

#[allow(clippy::too_many_arguments)]
fn progress_event_with_context(
    provider: Option<&str>,
    role: Option<&str>,
    stage: ProgressStage,
    status: Option<&str>,
    iteration: Option<usize>,
    total_iterations: Option<usize>,
    is_sub_agent: bool,
    duration_ms: Option<u128>,
    token_usage: Option<TokenUsage>,
    tool_calls: Vec<ToolUsage>,
    message: impl Into<String>,
) -> ProgressEvent {
    ProgressEvent {
        kind: runtime_event_kind(provider, stage, token_usage.as_ref(), &tool_calls),
        provider: provider.map(ToString::to_string),
        role: role.map(ToString::to_string),
        stage,
        status: status.map(ToString::to_string),
        iteration,
        total_iterations,
        is_sub_agent,
        duration_ms,
        token_usage,
        tool_calls,
        message: message.into(),
    }
}

fn runtime_event_kind(
    provider: Option<&str>,
    stage: ProgressStage,
    token_usage: Option<&TokenUsage>,
    tool_calls: &[ToolUsage],
) -> RuntimeEventKind {
    if !tool_calls.is_empty() {
        RuntimeEventKind::ToolUsage
    } else if token_usage.is_some() {
        RuntimeEventKind::TokenUsage
    } else if provider.is_some() {
        RuntimeEventKind::Provider
    } else if matches!(stage, ProgressStage::Context) {
        RuntimeEventKind::Context
    } else {
        RuntimeEventKind::Iteration
    }
}

fn command_progress(label: Option<&str>, sink: Option<ProgressSink>) -> Option<CommandProgress> {
    command_progress_with_input(label, sink, 0)
}

fn command_progress_with_input(
    label: Option<&str>,
    sink: Option<ProgressSink>,
    input_tokens: usize,
) -> Option<CommandProgress> {
    label.map(|label| CommandProgress {
        label: label.to_string(),
        sink,
        input_tokens,
    })
}

fn render_provider_capability_statuses(statuses: &[ProviderCapabilityStatus]) -> String {
    let mut lines = vec!["Provider capabilities".to_string()];
    for status in statuses {
        lines.push(format!(
            "- {} MCP: {} ({})",
            status.provider, status.mcp.status, status.mcp.command
        ));
        if !status.mcp.detail.trim().is_empty() {
            lines.push(format!("  {}", status.mcp.detail));
        }
        if let Some(skills) = &status.skills {
            lines.push(format!(
                "  skills/plugins: {} ({})",
                skills.status, skills.command
            ));
            if !skills.detail.trim().is_empty() {
                lines.push(format!("  {}", skills.detail));
            }
        }
        if let Some(tools) = &status.tools {
            lines.push(format!(
                "  tools/extensions: {} ({})",
                tools.status, tools.command
            ));
            if !tools.detail.trim().is_empty() {
                lines.push(format!("  {}", tools.detail));
            }
        }
        lines.push(format!("  {}", status.detail));
    }
    lines.join("\n")
}

fn run_social_login(resolved: &ResolvedArgs) -> Result<(), String> {
    let providers = if resolved.raw.auth_login_providers.is_empty() {
        resolved.members.clone()
    } else {
        resolved.raw.auth_login_providers.clone()
    };

    for provider in providers {
        validate_engine_name(&provider, false, "--auth-login-providers")?;
        let (bin, args, instruction): (String, Vec<String>, &str) =
            match provider.as_str() {
                "codex" => {
                    let mut args = vec!["login".to_string()];
                    if resolved.raw.auth_device_code {
                        args.push("--device-auth".to_string());
                    }
                    (
                        resolve_binary("codex"),
                        args,
                        "Complete the Codex browser login. Deeplinks and pasted codes are supported by the provider CLI when prompted.",
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
                        "Complete the Claude browser login. Paste any shown login code into this terminal when prompted; deeplinks are opened when the CLI emits them.",
                    )
                }
                "gemini" => (
                    resolve_binary("gemini"),
                    vec![],
                    "Use the Gemini CLI auth selector, choose browser/social login, and complete local callback or code paste when prompted.",
                ),
                _ => unreachable!(),
            };
        eprintln!(
            "[auth] launching {provider}: {}",
            format_command(&bin, &args)
        );
        eprintln!("[auth] {provider}: {instruction}");
        let result = run_interactive_auth_command(
            &bin,
            &args,
            &resolved.cwd,
            resolved.raw.auth_timeout * 1000,
            !resolved.raw.no_auth_open_browser,
            &provider,
        );
        if result.code.unwrap_or(1) != 0 {
            return Err(format!(
                "{provider} social login failed: {}",
                compact_failure(&result)
            ));
        }
        let status = provider_auth_status(&provider, resolved);
        eprintln!("[auth] {provider}: {} ({})", status.status, status.detail);
    }
    Ok(())
}

fn run_interactive_auth_command(
    command: &str,
    args: &[String],
    cwd: &Path,
    timeout_ms: u64,
    open_browser: bool,
    provider: &str,
) -> CommandResult {
    let started = Instant::now();
    let mut process = Command::new(command);
    process
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut process);
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            return CommandResult {
                command: command.to_string(),
                args: args.to_vec(),
                code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
                error: Some(error.to_string()),
                timeout_ms,
                duration_ms: started.elapsed().as_millis(),
            }
        }
    };

    let seen_urls = Arc::new(Mutex::new(HashSet::new()));
    let stdout = child.stdout.take().map(|pipe| {
        read_auth_pipe(
            pipe,
            true,
            provider.to_string(),
            open_browser,
            Arc::clone(&seen_urls),
        )
    });
    let stderr = child.stderr.take().map(|pipe| {
        read_auth_pipe(
            pipe,
            false,
            provider.to_string(),
            open_browser,
            Arc::clone(&seen_urls),
        )
    });
    let timeout = Duration::from_millis(timeout_ms);
    let mut timed_out = false;
    let code;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                code = status.code();
                break;
            }
            Ok(None) => {
                if timeout_ms > 0 && started.elapsed() >= timeout {
                    timed_out = true;
                    terminate_child_tree(&mut child);
                    let status = child.wait().ok();
                    code = status.and_then(|status| status.code());
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                return CommandResult {
                    command: command.to_string(),
                    args: args.to_vec(),
                    code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out,
                    cancelled: false,
                    error: Some(error.to_string()),
                    timeout_ms,
                    duration_ms: started.elapsed().as_millis(),
                }
            }
        }
    }

    CommandResult {
        command: command.to_string(),
        args: args.to_vec(),
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
        timeout_ms,
        duration_ms: started.elapsed().as_millis(),
    }
}

fn read_auth_pipe<R>(
    mut pipe: R,
    stdout: bool,
    provider: String,
    open_browser: bool,
    seen_urls: Arc<Mutex<HashSet<String>>>,
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
            if stdout {
                print!("{chunk}");
                let _ = io::stdout().flush();
            } else {
                eprint!("{chunk}");
                let _ = io::stderr().flush();
            }
            if open_browser {
                for url in extract_auth_urls(&chunk) {
                    let mut seen = seen_urls.lock().ok();
                    if seen.as_mut().is_some_and(|seen| !seen.insert(url.clone())) {
                        continue;
                    }
                    eprintln!("[auth] {provider}: opening {url}");
                    if let Err(error) = open_browser_url(&url) {
                        eprintln!("[auth] {provider}: failed to open {url}: {error}");
                    }
                }
            }
        }
        text
    })
}

fn extract_auth_urls(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|token| {
            token.trim_matches(|ch: char| {
                matches!(ch, '<' | '>' | '"' | '\'' | ')' | '(' | ',' | ';')
            })
        })
        .map(|token| token.trim_end_matches(['.', ':', ',', ';']))
        .filter(|token| {
            token.starts_with("http://")
                || token.starts_with("https://")
                || token.starts_with("codex://")
                || token.starts_with("openai://")
                || token.starts_with("claude://")
                || token.starts_with("anthropic://")
                || token.starts_with("gemini://")
                || token.starts_with("google://")
        })
        .map(ToString::to_string)
        .collect()
}

fn open_browser_url(url: &str) -> Result<(), String> {
    let (command, args): (&str, Vec<String>) = if cfg!(target_os = "macos") {
        ("open", vec![url.to_string()])
    } else if cfg!(target_os = "windows") {
        (
            "cmd",
            vec![
                "/c".to_string(),
                "start".to_string(),
                "".to_string(),
                url.to_string(),
            ],
        )
    } else {
        ("xdg-open", vec![url.to_string()])
    };
    Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|mut child| {
            let _ = child.try_wait();
        })
        .map_err(|error| error.to_string())
}

fn build_prompt_context_with_progress(
    resolved: &ResolvedArgs,
    progress: Option<ProgressSink>,
) -> Result<PromptContext, String> {
    let mut prompt = resolved.prompt.trim().to_string();
    let mut sections = Vec::new();
    let mut commands = Vec::new();

    for file in &resolved.raw.files {
        let path = if file.is_absolute() {
            file.clone()
        } else {
            resolved.cwd.join(file)
        };
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read tagged file {}: {error}", path.display()))?;
        sections.push(format!("--- file: {} ---\n{}", file.display(), content));
    }

    for command in &resolved.raw.commands {
        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        let args = if cfg!(windows) {
            vec!["/C".to_string(), command.clone()]
        } else {
            vec!["-lc".to_string(), command.clone()]
        };
        let live_label = (resolved.raw.verbose || progress.is_some())
            .then(|| format!("context command `{}`", truncate(command, 80)));
        emit_runtime_event(
            &progress,
            false,
            progress_event(
                None,
                None,
                ProgressStage::Context,
                None,
                format!("[amon-hen] context command `{}`", truncate(command, 80)),
            ),
        );
        let result = run_command(
            CommandRequest::new(shell, &args, &resolved.cwd, resolved.raw.timeout * 1000)
                .progress(command_progress(live_label.as_deref(), progress.clone())),
        );
        let telemetry = command_telemetry(&result);
        let mut event = progress_event(
            None,
            None,
            ProgressStage::Done,
            Some(&telemetry.status),
            format!(
                "[amon-hen] context command `{}` {} in {:.1}s",
                truncate(command, 80),
                telemetry.status,
                result.duration_ms as f64 / 1000.0
            ),
        );
        event.kind = RuntimeEventKind::Context;
        emit_runtime_event(&progress, false, event);
        commands.push(telemetry);
        sections.push(format!(
            "--- command: {} (exit {}) ---\nstdout:\n{}\nstderr:\n{}",
            command,
            result
                .code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            result.stdout.trim(),
            result.stderr.trim()
        ));
    }

    if !sections.is_empty() {
        prompt.push_str("\n\nPrompt context:\n");
        prompt.push_str(&sections.join("\n\n"));
    }
    Ok(PromptContext { prompt, commands })
}

fn run_amon_hen_with_progress(
    resolved: &ResolvedArgs,
    query: String,
    prompt_commands: Vec<CommandTelemetry>,
    progress: Option<ProgressSink>,
) -> AmonHenResult {
    run_amon_hen_with_progress_and_cancel(resolved, query, prompt_commands, progress, None)
}

fn run_amon_hen_with_progress_and_cancel(
    resolved: &ResolvedArgs,
    query: String,
    prompt_commands: Vec<CommandTelemetry>,
    progress: Option<ProgressSink>,
    cancel: Option<Arc<AtomicBool>>,
) -> AmonHenResult {
    let workflow = build_workflow(resolved);
    let mut previous_iteration = Vec::new();
    let mut final_members = Vec::new();
    let mut iterations = Vec::new();

    for iteration in 1..=workflow.iterations {
        let iteration_started = Instant::now();
        let handoff_context =
            iteration_handoff_context(&previous_iteration, resolved.raw.max_member_chars);
        emit_runtime_event(
            &progress,
            resolved.raw.verbose,
            progress_event_with_context(
                None,
                None,
                ProgressStage::Start,
                None,
                Some(iteration),
                Some(workflow.iterations),
                false,
                None,
                None,
                vec![],
                format!(
                    "[amon-hen] iteration {iteration}/{} started",
                    workflow.iterations
                ),
            ),
        );
        final_members = run_iteration(
            resolved,
            &query,
            &workflow,
            iteration,
            &previous_iteration,
            progress.clone(),
            cancel.clone(),
        );
        iterations.push(iteration_record(
            iteration,
            workflow.iterations,
            final_members.clone(),
            iteration_started.elapsed().as_millis(),
            handoff_context,
            None,
        ));
        previous_iteration = final_members.clone();
    }

    let successes = final_members
        .iter()
        .filter(|result| result.status == "ok")
        .cloned()
        .collect::<Vec<_>>();
    let summary = if successes.is_empty() {
        EngineResult {
            name: resolved.raw.summarizer.clone(),
            bin: None,
            status: "error".to_string(),
            duration_ms: 0,
            detail: "No Amon Hen member produced a response.".to_string(),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            output: String::new(),
            command: String::new(),
            token_usage: token_usage("", ""),
            tool_calls: vec![],
            sub_agents: vec![],
            role: "summary".to_string(),
            iteration: workflow.iterations,
            total_iterations: workflow.iterations,
            team_size: 0,
        }
    } else {
        let summary_prompt =
            build_summary_prompt(&query, &successes, &workflow, resolved.raw.max_member_chars);
        if let Some(record) = iterations.last_mut() {
            record.summary_context = Some(truncate(&summary_prompt, resolved.raw.max_member_chars));
        }
        let summarizer = pick_summarizer(resolved, &successes);
        let mut options = engine_options(
            resolved,
            EngineOptionsInput {
                member: &summarizer,
                prompt: summary_prompt,
                role: "summary",
                iteration: workflow.iterations,
                workflow: &workflow,
                progress: progress.clone(),
                cancel: cancel.clone(),
            },
        );
        options.role = "summary".to_string();
        options.team_size = 0;
        run_engine(&summarizer, options)
    };

    AmonHenResult {
        query,
        cwd: resolved.cwd.display().to_string(),
        members_requested: resolved.members.clone(),
        summarizer_requested: resolved.raw.summarizer.clone(),
        workflow,
        prompt_commands,
        iterations,
        members: final_members,
        summary,
    }
}

fn build_workflow(resolved: &ResolvedArgs) -> Workflow {
    let mut teams = HashMap::new();
    for member in ENGINES {
        teams.insert(member.to_string(), resolved.raw.team_work);
    }
    if let Some(value) = resolved.raw.codex_sub_agents {
        teams.insert("codex".to_string(), value);
    }
    if let Some(value) = resolved.raw.claude_sub_agents {
        teams.insert("claude".to_string(), value);
    }
    if let Some(value) = resolved.raw.gemini_sub_agents {
        teams.insert("gemini".to_string(), value);
    }
    Workflow {
        handoff: resolved.raw.handoff || resolved.raw.planner_mode == PLANNER_MODE_REVIEW_CHAIN,
        lead: resolved.raw.lead.clone(),
        planner: resolved.raw.planner.clone(),
        planner_mode: resolved.raw.planner_mode.clone(),
        iterations: resolved.raw.iterations.max(1),
        team_work: resolved.raw.team_work,
        teams,
    }
}

fn effective_team_size(workflow: &Workflow, member: &str) -> usize {
    *workflow.teams.get(member).unwrap_or(&workflow.team_work)
}

fn iteration_record(
    iteration: usize,
    total_iterations: usize,
    members: Vec<EngineResult>,
    duration_ms: u128,
    handoff_context: Option<String>,
    summary_context: Option<String>,
) -> IterationRecord {
    let status = if members.iter().any(|member| member.status == "ok") {
        "ok"
    } else if members.iter().any(|member| member.status == "cancelled") {
        "cancelled"
    } else if members.iter().any(|member| member.status == "timeout") {
        "timeout"
    } else if members.iter().any(|member| member.status == "missing") {
        "missing"
    } else {
        "error"
    };
    let sub_agents = members
        .iter()
        .flat_map(|member| member.sub_agents.iter().cloned())
        .collect::<Vec<_>>();
    let token_usage = aggregate_results_token_usage(members.iter());
    let tool_calls = members
        .iter()
        .chain(sub_agents.iter())
        .flat_map(|member| member.tool_calls.iter().cloned())
        .collect::<Vec<_>>();
    IterationRecord {
        iteration,
        total_iterations,
        status: status.to_string(),
        duration_ms,
        members,
        sub_agents,
        handoff_context,
        summary_context,
        token_usage,
        tool_calls,
    }
}

fn iteration_handoff_context(
    previous_iteration: &[EngineResult],
    max_member_chars: usize,
) -> Option<String> {
    let context = previous_iteration
        .iter()
        .filter(|result| result.status == "ok" && !result.output.trim().is_empty())
        .map(|result| {
            format!(
                "### {} ({})\n{}",
                result.name,
                result.role,
                truncate(result.output.trim(), max_member_chars)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    (!context.is_empty()).then_some(context)
}

fn run_iteration(
    resolved: &ResolvedArgs,
    query: &str,
    workflow: &Workflow,
    iteration: usize,
    previous_iteration: &[EngineResult],
    progress: Option<ProgressSink>,
    cancel: Option<Arc<AtomicBool>>,
) -> Vec<EngineResult> {
    let ordered = execution_order(
        &resolved.members,
        workflow.planner.as_deref(),
        workflow.lead.as_deref(),
    );
    if workflow.handoff && planner_mode_runs_serial(workflow) {
        let mut results = Vec::new();
        for member in ordered {
            let role = role_for(&member, workflow);
            let team_size = effective_team_size(workflow, &member);
            let prompt = build_member_prompt(MemberPromptInput {
                query,
                role: &role,
                workflow,
                iteration,
                team_size,
                previous_iteration,
                handoff_results: &results,
                plan_output: "",
            });
            let options = engine_options(
                resolved,
                EngineOptionsInput {
                    member: &member,
                    prompt,
                    role: &role,
                    iteration,
                    workflow,
                    progress: progress.clone(),
                    cancel: cancel.clone(),
                },
            );
            results.push(run_engine(&member, options));
        }
        return results;
    }

    let planner = workflow
        .planner
        .as_ref()
        .filter(|planner| ordered.contains(planner))
        .cloned();
    let mut results = Vec::new();
    let mut plan_output = String::new();
    let blocking_planner = workflow.planner_mode == PLANNER_MODE_BLOCKING;
    if let Some(planner_name) = planner.as_ref().filter(|_| blocking_planner) {
        let role = role_for(planner_name, workflow);
        let team_size = effective_team_size(workflow, planner_name);
        let prompt = build_member_prompt(MemberPromptInput {
            query,
            role: &role,
            workflow,
            iteration,
            team_size,
            previous_iteration,
            handoff_results: &[],
            plan_output: "",
        });
        let options = engine_options(
            resolved,
            EngineOptionsInput {
                member: planner_name,
                prompt,
                role: &role,
                iteration,
                workflow,
                progress: progress.clone(),
                cancel: cancel.clone(),
            },
        );
        let result = run_engine(planner_name, options);
        if result.status == "ok" {
            plan_output = result.output.clone();
        }
        results.push(result);
    }

    let (tx, rx) = mpsc::channel();
    let executors = ordered
        .into_iter()
        .filter(|member| !(blocking_planner && Some(member) == planner.as_ref()))
        .collect::<Vec<_>>();
    let resolved = Arc::new(resolved.clone());
    let workflow = Arc::new(workflow.clone());
    let previous_iteration = Arc::new(previous_iteration.to_vec());
    let plan_output = Arc::new(plan_output);
    let progress = Arc::new(progress);
    let cancel = Arc::new(cancel);
    for member in executors.clone() {
        let tx = tx.clone();
        let resolved = Arc::clone(&resolved);
        let workflow = Arc::clone(&workflow);
        let query = query.to_string();
        let previous_iteration = Arc::clone(&previous_iteration);
        let plan_output = Arc::clone(&plan_output);
        let progress = Arc::clone(&progress);
        let cancel = Arc::clone(&cancel);
        thread::spawn(move || {
            let role = role_for(&member, &workflow);
            let team_size = effective_team_size(&workflow, &member);
            let prompt = build_member_prompt(MemberPromptInput {
                query: &query,
                role: &role,
                workflow: &workflow,
                iteration,
                team_size,
                previous_iteration: previous_iteration.as_slice(),
                handoff_results: &[],
                plan_output: &plan_output,
            });
            let options = engine_options(
                &resolved,
                EngineOptionsInput {
                    member: &member,
                    prompt,
                    role: &role,
                    iteration,
                    workflow: &workflow,
                    progress: (*progress).clone(),
                    cancel: (*cancel).clone(),
                },
            );
            let _ = tx.send(run_engine(&member, options));
        });
    }
    drop(tx);
    let mut executor_results = rx.into_iter().collect::<Vec<_>>();
    executor_results.sort_by_key(|result| {
        executors
            .iter()
            .position(|member| member == &result.name)
            .unwrap_or(usize::MAX)
    });
    results.extend(executor_results);
    results
}

fn planner_mode_runs_serial(workflow: &Workflow) -> bool {
    matches!(
        workflow.planner_mode.as_str(),
        PLANNER_MODE_BLOCKING | PLANNER_MODE_REVIEW_CHAIN
    )
}

fn engine_options(resolved: &ResolvedArgs, input: EngineOptionsInput<'_>) -> EngineRunOptions {
    EngineRunOptions {
        prompt: input.prompt,
        cwd: resolved.cwd.clone(),
        timeout_ms: resolved.raw.timeout * 1000,
        effort: provider_effort(resolved, input.member),
        model: provider_model(resolved, input.member),
        permission: provider_permission(resolved, input.member),
        auth: provider_auth(resolved, input.member),
        capability: provider_capability(resolved, input.member),
        role: input.role.to_string(),
        iteration: input.iteration,
        total_iterations: input.workflow.iterations,
        team_size: effective_team_size(input.workflow, input.member),
        is_sub_agent: false,
        live: resolved.raw.verbose || input.progress.is_some(),
        progress: input.progress,
        cancel: input.cancel,
    }
}

fn provider_effort(resolved: &ResolvedArgs, member: &str) -> Option<String> {
    provider_option(
        member,
        &resolved.raw.codex_effort,
        &resolved.raw.claude_effort,
        &resolved.raw.gemini_effort,
    )
    .or_else(|| {
        resolved
            .raw
            .effort
            .map(|effort| effort.as_str().to_string())
    })
}

fn provider_model(resolved: &ResolvedArgs, member: &str) -> Option<String> {
    provider_option(
        member,
        &resolved.raw.codex_model,
        &resolved.raw.claude_model,
        &resolved.raw.gemini_model,
    )
}

fn provider_permission(resolved: &ResolvedArgs, member: &str) -> Option<String> {
    match Engine::parse(member) {
        Some(Engine::Codex) => Some(resolved.raw.codex_sandbox.clone()),
        Some(Engine::Claude) => Some(resolved.raw.claude_permission_mode.clone()),
        Some(Engine::Gemini) => Some(resolved.raw.gemini_approval_mode.clone()),
        None => None,
    }
}

fn provider_auth(resolved: &ResolvedArgs, member: &str) -> String {
    match Engine::parse(member) {
        Some(Engine::Codex) => resolved.raw.codex_auth.clone(),
        Some(Engine::Claude) => resolved.raw.claude_auth.clone(),
        Some(Engine::Gemini) => resolved.raw.gemini_auth.clone(),
        None => DEFAULT_AUTH_MODE.to_string(),
    }
}

fn provider_capability(resolved: &ResolvedArgs, member: &str) -> ProviderCapability {
    match Engine::parse(member).expect("provider capabilities use validated engines") {
        Engine::Codex => ProviderCapability {
            mode: inferred_capability_mode(
                &resolved.raw.codex_capabilities,
                !resolved.raw.codex_config.is_empty() || resolved.raw.codex_mcp_profile.is_some(),
            ),
            config: resolved.raw.codex_config.clone(),
            mcp_profile: resolved.raw.codex_mcp_profile.clone(),
            mcp_config: vec![],
            allowed_tools: vec![],
            disallowed_tools: vec![],
            tools: vec![],
            agent: None,
            agents_json: None,
            plugin_dirs: vec![],
            strict_mcp_config: false,
            disable_slash_commands: false,
            settings: None,
            tools_profile: vec![],
            allowed_mcp_servers: vec![],
            policy: vec![],
            admin_policy: vec![],
        },
        Engine::Claude => ProviderCapability {
            mode: inferred_capability_mode(
                &resolved.raw.claude_capabilities,
                !resolved.raw.claude_mcp_config.is_empty()
                    || !resolved.raw.claude_allowed_tools.is_empty()
                    || !resolved.raw.claude_disallowed_tools.is_empty()
                    || !resolved.raw.claude_tools.is_empty()
                    || resolved.raw.claude_agent.is_some()
                    || resolved.raw.claude_agents_json.is_some()
                    || !resolved.raw.claude_plugin_dir.is_empty()
                    || resolved.raw.claude_strict_mcp_config
                    || resolved.raw.claude_disable_slash_commands,
            ),
            config: vec![],
            mcp_profile: None,
            mcp_config: resolved.raw.claude_mcp_config.clone(),
            allowed_tools: resolved.raw.claude_allowed_tools.clone(),
            disallowed_tools: resolved.raw.claude_disallowed_tools.clone(),
            tools: resolved.raw.claude_tools.clone(),
            agent: resolved.raw.claude_agent.clone(),
            agents_json: resolved.raw.claude_agents_json.clone(),
            plugin_dirs: resolved.raw.claude_plugin_dir.clone(),
            strict_mcp_config: resolved.raw.claude_strict_mcp_config,
            disable_slash_commands: resolved.raw.claude_disable_slash_commands,
            settings: None,
            tools_profile: vec![],
            allowed_mcp_servers: vec![],
            policy: vec![],
            admin_policy: vec![],
        },
        Engine::Gemini => ProviderCapability {
            mode: inferred_capability_mode(
                &resolved.raw.gemini_capabilities,
                resolved.raw.gemini_settings.is_some()
                    || !resolved.raw.gemini_tools_profile.is_empty()
                    || !resolved.raw.gemini_allowed_mcp_servers.is_empty()
                    || !resolved.raw.gemini_policy.is_empty()
                    || !resolved.raw.gemini_admin_policy.is_empty(),
            ),
            config: vec![],
            mcp_profile: None,
            mcp_config: vec![],
            allowed_tools: vec![],
            disallowed_tools: vec![],
            tools: vec![],
            agent: None,
            agents_json: None,
            plugin_dirs: vec![],
            strict_mcp_config: false,
            disable_slash_commands: false,
            settings: resolved.raw.gemini_settings.clone(),
            tools_profile: resolved.raw.gemini_tools_profile.clone(),
            allowed_mcp_servers: resolved.raw.gemini_allowed_mcp_servers.clone(),
            policy: resolved.raw.gemini_policy.clone(),
            admin_policy: resolved.raw.gemini_admin_policy.clone(),
        },
    }
}

fn inferred_capability_mode(configured: &str, has_override_flags: bool) -> String {
    if configured == CAPABILITY_INHERIT && has_override_flags {
        CAPABILITY_OVERRIDE.to_string()
    } else {
        configured.to_string()
    }
}

fn provider_option(
    member: &str,
    codex: &Option<String>,
    claude: &Option<String>,
    gemini: &Option<String>,
) -> Option<String> {
    match Engine::parse(member) {
        Some(Engine::Codex) => codex.clone(),
        Some(Engine::Claude) => claude.clone(),
        Some(Engine::Gemini) => gemini.clone(),
        None => None,
    }
}

fn build_sub_agent_prompt(original: &str, role: &str, index: usize, total: usize) -> String {
    format!(
        "You are sub-agent {index} of {total} for an Amon Hen provider assigned role `{role}`.\n\
         Work independently on a useful slice of the task. Inspect, reason, or verify as needed, \
         then return concise findings, risks, and concrete recommendations for the provider lead.\n\n\
         Original provider prompt:\n{original}"
    )
}

fn build_team_lead_prompt(original: &str, sub_agents: &[EngineResult]) -> String {
    let handoff = sub_agents
        .iter()
        .map(|agent| {
            format!(
                "### {} [{}]\n{}",
                agent.role,
                agent.status,
                if agent.output.trim().is_empty() {
                    agent.detail.trim()
                } else {
                    agent.output.trim()
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "You are the provider lead. Use the sub-agent handoffs below, resolve disagreements, \
         and produce the final provider response for the original Amon Hen role.\n\n\
         Sub-agent handoffs:\n{handoff}\n\nOriginal provider prompt:\n{original}"
    )
}

fn run_engine(name: &str, options: EngineRunOptions) -> EngineResult {
    if let Some(result) = missing_engine_if_unavailable(name, &options) {
        return result;
    }
    if options.team_size > 0 && options.role != "summary" && !options.is_sub_agent {
        return run_engine_team(name, options);
    }
    run_engine_single(name, options, vec![])
}

fn missing_engine_if_unavailable(name: &str, options: &EngineRunOptions) -> Option<EngineResult> {
    let bin = resolve_binary(name);
    if command_available(&bin) {
        return None;
    }
    let detail = format!(
        "Provider CLI `{bin}` was not found in PATH. Install the {name} CLI on this machine or set {} to the executable path.",
        Engine::parse(name)
            .map(Engine::binary_env_var)
            .unwrap_or("AMON_HEN_PROVIDER_BIN")
    );
    let message = format!("[amon-hen] {name} missing: {detail}");
    emit_runtime_event(
        &options.progress,
        options.live && options.progress.is_none(),
        progress_event_with_context(
            Some(name),
            Some(&options.role),
            ProgressStage::Done,
            Some("missing"),
            Some(options.iteration),
            Some(options.total_iterations),
            options.is_sub_agent,
            Some(0),
            Some(token_usage(&options.prompt, "")),
            vec![],
            message,
        ),
    );
    Some(EngineResult {
        name: name.to_string(),
        bin: Some(bin.clone()),
        status: "missing".to_string(),
        duration_ms: 0,
        detail: detail.clone(),
        exit_code: None,
        stdout: String::new(),
        stderr: detail,
        output: String::new(),
        command: bin,
        token_usage: token_usage(&options.prompt, ""),
        tool_calls: vec![],
        sub_agents: vec![],
        role: options.role.clone(),
        iteration: options.iteration,
        total_iterations: options.total_iterations,
        team_size: options.team_size,
    })
}

fn command_available(command: &str) -> bool {
    let path = Path::new(command);
    if path.components().count() > 1 || path.is_absolute() {
        return path.is_file();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| dir.join(command).is_file())
}

fn run_engine_team(name: &str, options: EngineRunOptions) -> EngineResult {
    let team_size = options.team_size;
    let (tx, rx) = mpsc::channel();
    for index in 1..=team_size {
        let tx = tx.clone();
        let name = name.to_string();
        let mut sub_options = options.clone();
        sub_options.team_size = 0;
        sub_options.is_sub_agent = true;
        sub_options.role = format!("{}:sub-agent-{index}", options.role);
        sub_options.prompt =
            build_sub_agent_prompt(&options.prompt, &options.role, index, team_size);
        thread::spawn(move || {
            let _ = tx.send((index, run_engine_single(&name, sub_options, vec![])));
        });
    }
    drop(tx);

    let mut indexed = rx.into_iter().collect::<Vec<_>>();
    indexed.sort_by_key(|(index, _)| *index);
    let sub_agents = indexed
        .into_iter()
        .map(|(_, result)| result)
        .collect::<Vec<_>>();
    let mut lead_options = options.clone();
    lead_options.prompt = build_team_lead_prompt(&options.prompt, &sub_agents);
    lead_options.is_sub_agent = true;
    run_engine_single(name, lead_options, sub_agents)
}

fn run_engine_single(
    name: &str,
    options: EngineRunOptions,
    sub_agents: Vec<EngineResult>,
) -> EngineResult {
    let started = Instant::now();
    let bin = resolve_binary(name);
    let live = options.live;
    let role_is_sub_agent = options.role.contains(":sub-agent-");
    let start_message = format!(
        "[amon-hen] start {} role={} iteration={}/{}{}",
        name,
        options.role,
        options.iteration,
        options.total_iterations,
        if role_is_sub_agent { " sub-agent" } else { "" }
    );
    emit_runtime_event(
        &options.progress,
        live && options.progress.is_none(),
        progress_event_with_context(
            Some(name),
            Some(&options.role),
            ProgressStage::Start,
            Some("running"),
            Some(options.iteration),
            Some(options.total_iterations),
            role_is_sub_agent,
            None,
            None,
            vec![],
            start_message,
        ),
    );
    let result = match Engine::parse(name) {
        Some(Engine::Codex) => run_codex(&bin, &options),
        Some(Engine::Claude) => run_claude(&bin, &options),
        Some(Engine::Gemini) => run_gemini(&bin, &options),
        None => CommandResult {
            command: name.to_string(),
            args: vec![],
            code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
            error: Some(format!("Unknown engine: {name}")),
            timeout_ms: options.timeout_ms,
            duration_ms: started.elapsed().as_millis(),
        },
    };
    let engine_progress = options.progress.clone();
    let engine = finalize_engine(
        name,
        &bin,
        started.elapsed().as_millis(),
        result,
        options,
        sub_agents,
    );
    if live {
        let done_message = format!(
            "[amon-hen] done {} role={} status={} elapsed={:.1}s tokens={} tools={} sub-agents={}",
            engine.name,
            engine.role,
            engine.status,
            engine.duration_ms as f64 / 1000.0,
            engine.token_usage.total,
            engine.tool_calls.len(),
            engine.sub_agents.len()
        );
        emit_runtime_event(
            &engine_progress,
            live && engine_progress.is_none(),
            progress_event_with_context(
                Some(&engine.name),
                Some(&engine.role),
                ProgressStage::Done,
                Some(&engine.status),
                Some(engine.iteration),
                Some(engine.total_iterations),
                engine.role.contains(":sub-agent-"),
                Some(engine.duration_ms),
                Some(engine.token_usage.clone()),
                engine.tool_calls.clone(),
                done_message,
            ),
        );
    }
    engine
}

fn push_arg(args: &mut Vec<String>, flag: &str, value: impl Into<String>) {
    args.push(flag.to_string());
    args.push(value.into());
}

fn push_optional_arg(args: &mut Vec<String>, flag: &str, value: &Option<String>) {
    if let Some(value) = value {
        push_arg(args, flag, value.clone());
    }
}

fn push_repeated_flag(args: &mut Vec<String>, flag: &str, values: &[String]) {
    if !values.is_empty() {
        args.push(flag.to_string());
        args.extend(values.iter().cloned());
    }
}

fn push_each_arg(args: &mut Vec<String>, flag: &str, values: &[String]) {
    for value in values {
        push_arg(args, flag, value.clone());
    }
}

fn run_codex(bin: &str, options: &EngineRunOptions) -> CommandResult {
    let temp = match tempfile::tempdir() {
        Ok(temp) => temp,
        Err(error) => {
            return CommandResult {
                command: bin.to_string(),
                args: vec![],
                code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
                error: Some(error.to_string()),
                timeout_ms: options.timeout_ms,
                duration_ms: 0,
            }
        }
    };
    let output_path = temp.path().join("last-message.txt");
    let mut args = vec!["exec".to_string()];
    push_optional_arg(&mut args, "--model", &options.model);
    if let Some(effort) = &options.effort {
        push_arg(&mut args, "-c", format!("model_reasoning_effort={effort}"));
    }
    if options.capability.mode == CAPABILITY_OVERRIDE {
        for config in &options.capability.config {
            push_arg(&mut args, "-c", config.clone());
        }
        push_optional_arg(&mut args, "--profile", &options.capability.mcp_profile);
    }
    args.extend([
        "--skip-git-repo-check".to_string(),
        "--sandbox".to_string(),
        options
            .permission
            .clone()
            .unwrap_or_else(|| "read-only".to_string()),
        "--ephemeral".to_string(),
        "--json".to_string(),
        "-o".to_string(),
        output_path.display().to_string(),
        "-".to_string(),
    ]);
    let mut result = run_command(
        CommandRequest::new(bin, &args, &options.cwd, options.timeout_ms)
            .stdin_text(Some(&options.prompt))
            .progress(command_progress_with_input(
                live_label("codex", options).as_deref(),
                options.progress.clone(),
                estimate_tokens(&options.prompt),
            ))
            .cancel(options.cancel.clone()),
    );
    if let Ok(output) = fs::read_to_string(output_path) {
        if !output.trim().is_empty() {
            result.stdout = output;
        }
    }
    result
}

fn run_claude(bin: &str, options: &EngineRunOptions) -> CommandResult {
    let mut args = Vec::new();
    if should_use_claude_bare_mode(&options.auth) {
        args.push("--bare".to_string());
    }
    args.extend([
        "-p".to_string(),
        "--permission-mode".to_string(),
        options
            .permission
            .clone()
            .unwrap_or_else(|| "plan".to_string()),
        "--verbose".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--include-partial-messages".to_string(),
        "--no-session-persistence".to_string(),
    ]);
    push_optional_arg(&mut args, "--model", &options.model);
    if let Some(effort) = options
        .effort
        .clone()
        .or_else(|| std::env::var("CLAUDE_CODE_EFFORT_LEVEL").ok())
    {
        push_arg(&mut args, "--effort", effort);
    }
    if options.capability.mode == CAPABILITY_OVERRIDE {
        push_repeated_flag(&mut args, "--mcp-config", &options.capability.mcp_config);
        push_repeated_flag(
            &mut args,
            "--allowedTools",
            &options.capability.allowed_tools,
        );
        push_repeated_flag(
            &mut args,
            "--disallowedTools",
            &options.capability.disallowed_tools,
        );
        push_repeated_flag(&mut args, "--tools", &options.capability.tools);
        push_optional_arg(&mut args, "--agent", &options.capability.agent);
        push_optional_arg(&mut args, "--agents", &options.capability.agents_json);
        push_each_arg(&mut args, "--plugin-dir", &options.capability.plugin_dirs);
        if options.capability.strict_mcp_config {
            args.push("--strict-mcp-config".to_string());
        }
        if options.capability.disable_slash_commands {
            args.push("--disable-slash-commands".to_string());
        }
    }
    run_command(
        CommandRequest::new(bin, &args, &options.cwd, options.timeout_ms)
            .stdin_text(Some(&options.prompt))
            .progress(command_progress_with_input(
                live_label("claude", options).as_deref(),
                options.progress.clone(),
                estimate_tokens(&options.prompt),
            ))
            .cancel(options.cancel.clone()),
    )
}

fn run_gemini(bin: &str, options: &EngineRunOptions) -> CommandResult {
    let mut args = Vec::new();
    push_optional_arg(&mut args, "--model", &options.model);
    if options.capability.mode == CAPABILITY_OVERRIDE {
        push_repeated_flag(&mut args, "--extensions", &options.capability.tools_profile);
        push_repeated_flag(
            &mut args,
            "--allowed-mcp-server-names",
            &options.capability.allowed_mcp_servers,
        );
        push_repeated_flag(&mut args, "--policy", &options.capability.policy);
        push_repeated_flag(
            &mut args,
            "--admin-policy",
            &options.capability.admin_policy,
        );
    }
    args.extend([
        "-p".to_string(),
        String::new(),
        "--skip-trust".to_string(),
        "--approval-mode".to_string(),
        options
            .permission
            .clone()
            .unwrap_or_else(|| "plan".to_string()),
        "--output-format".to_string(),
        "json".to_string(),
    ]);
    let mut envs = HashMap::new();
    let effort_settings = prepare_gemini_settings(options);
    if let Some(path) = effort_settings.as_ref() {
        envs.insert(
            "GEMINI_CLI_SYSTEM_SETTINGS_PATH".to_string(),
            path.path.display().to_string(),
        );
    } else if options.capability.mode == CAPABILITY_OVERRIDE {
        if let Some(settings) = &options.capability.settings {
            envs.insert(
                "GEMINI_CLI_SYSTEM_SETTINGS_PATH".to_string(),
                settings.clone(),
            );
        }
    }
    run_command(
        CommandRequest::new(bin, &args, &options.cwd, options.timeout_ms)
            .envs(envs)
            .stdin_text(Some(&options.prompt))
            .progress(command_progress_with_input(
                live_label("gemini", options).as_deref(),
                options.progress.clone(),
                estimate_tokens(&options.prompt),
            ))
            .cancel(options.cancel.clone()),
    )
}

fn live_label(name: &str, options: &EngineRunOptions) -> Option<String> {
    options.live.then(|| {
        format!(
            "{} {} iteration {}/{}",
            name, options.role, options.iteration, options.total_iterations
        )
    })
}

fn provider_from_live_label(label: &str) -> Option<&str> {
    let provider = label.split_whitespace().next()?;
    ENGINES.contains(&provider).then_some(provider)
}

fn role_from_live_label(label: &str) -> Option<&str> {
    if provider_from_live_label(label).is_some() {
        return label.split_whitespace().nth(1);
    }
    None
}

fn iteration_from_live_label(label: &str) -> Option<usize> {
    iteration_pair_from_live_label(label).map(|(iteration, _)| iteration)
}

fn total_iterations_from_live_label(label: &str) -> Option<usize> {
    iteration_pair_from_live_label(label).map(|(_, total)| total)
}

fn iteration_pair_from_live_label(label: &str) -> Option<(usize, usize)> {
    let pair = label.split_whitespace().last()?;
    let (iteration, total) = pair.split_once('/')?;
    Some((iteration.parse().ok()?, total.parse().ok()?))
}

fn prepare_gemini_settings(options: &EngineRunOptions) -> Option<TempSettings> {
    let effort = options.effort.as_deref()?;
    let budget = match effort {
        "low" => 1024,
        "medium" => 8192,
        "high" => 24576,
        _ => return None,
    };
    let dir = tempfile::tempdir().ok()?;
    let path = dir.path().join("settings.json");
    let mut settings = if options.capability.mode == CAPABILITY_OVERRIDE {
        options
            .capability
            .settings
            .as_ref()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .unwrap_or_else(|| Value::Object(Default::default()))
    } else {
        Value::Object(Default::default())
    };
    if let Value::Object(object) = &mut settings {
        object.insert("thinkingBudget".to_string(), Value::from(budget));
    }
    let _ = fs::write(&path, serde_json::to_vec(&settings).ok()?);
    Some(TempSettings { _dir: dir, path })
}

fn should_use_claude_bare_mode(auth: &str) -> bool {
    if auth == "api-key" {
        return true;
    }
    if matches!(auth, "social-login" | "oauth" | "keychain") {
        return false;
    }
    if std::env::var("CLAUDE_CODE_OAUTH_TOKEN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .is_some()
    {
        return false;
    }
    std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .is_some()
}

fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

fn terminate_child_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        let pgid = -(child.id() as i32);
        unsafe {
            libc::kill(pgid, libc::SIGTERM);
        }
        for _ in 0..20 {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        unsafe {
            libc::kill(pgid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

fn run_command(request: CommandRequest<'_>) -> CommandResult {
    let CommandRequest {
        command,
        args,
        cwd,
        stdin_text,
        timeout_ms,
        envs,
        progress,
        cancel,
    } = request;
    let started = Instant::now();
    if let Some(progress) = &progress {
        emit_runtime_event(
            &progress.sink,
            progress.sink.is_none(),
            progress_event_with_context(
                provider_from_live_label(&progress.label),
                role_from_live_label(&progress.label),
                ProgressStage::Spawn,
                Some("running"),
                iteration_from_live_label(&progress.label),
                total_iterations_from_live_label(&progress.label),
                role_from_live_label(&progress.label)
                    .is_some_and(|role| role.contains(":sub-agent-")),
                None,
                None,
                vec![],
                format!("[amon-hen] spawn {}", progress.label),
            ),
        );
    }
    let mut process = Command::new(command);
    process
        .args(args)
        .current_dir(cwd)
        .envs(envs)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut process);
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            let status = spawn_error_status(&error);
            if let Some(progress) = &progress {
                emit_runtime_event(
                    &progress.sink,
                    progress.sink.is_none(),
                    progress_event_with_context(
                        provider_from_live_label(&progress.label),
                        role_from_live_label(&progress.label),
                        ProgressStage::Done,
                        Some(status),
                        iteration_from_live_label(&progress.label),
                        total_iterations_from_live_label(&progress.label),
                        role_from_live_label(&progress.label)
                            .is_some_and(|role| role.contains(":sub-agent-")),
                        Some(started.elapsed().as_millis()),
                        None,
                        vec![],
                        format!("[amon-hen] spawn failed {}: {error}", progress.label),
                    ),
                );
            }
            return CommandResult {
                command: command.to_string(),
                args: args.to_vec(),
                code: None,
                stdout: String::new(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
                error: Some(error.to_string()),
                timeout_ms,
                duration_ms: started.elapsed().as_millis(),
            };
        }
    };

    let stdout = child
        .stdout
        .take()
        .map(|pipe| read_pipe(pipe, progress.clone(), "stdout"));
    let stderr = child
        .stderr
        .take()
        .map(|pipe| read_pipe(pipe, progress.clone(), "stderr"));
    let mut stdin_writer = child.stdin.take().and_then(|mut stdin| {
        stdin_text.map(|text| {
            let text = text.to_string();
            thread::spawn(move || {
                let _ = stdin.write_all(text.as_bytes());
            })
        })
    });
    let timeout = Duration::from_millis(timeout_ms);
    let mut next_live_tick = Duration::from_secs(10);
    let mut timed_out = false;
    let mut cancelled = false;
    let code;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                code = status.code();
                break;
            }
            Ok(None) => {
                if cancel
                    .as_ref()
                    .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
                {
                    cancelled = true;
                    terminate_child_tree(&mut child);
                    let status = child.wait().ok();
                    code = status.and_then(|status| status.code());
                    break;
                }
                if timeout_ms > 0 && started.elapsed() >= timeout {
                    timed_out = true;
                    terminate_child_tree(&mut child);
                    let status = child.wait().ok();
                    code = status.and_then(|status| status.code());
                    break;
                }
                if let Some(progress) = &progress {
                    let elapsed = started.elapsed();
                    if elapsed >= next_live_tick {
                        let timeout_detail = if timeout_ms > 0 {
                            format!("/{}s", timeout_ms / 1000)
                        } else {
                            String::new()
                        };
                        emit_runtime_event(
                            &progress.sink,
                            progress.sink.is_none(),
                            progress_event_with_context(
                                provider_from_live_label(&progress.label),
                                role_from_live_label(&progress.label),
                                ProgressStage::Heartbeat,
                                Some("running"),
                                iteration_from_live_label(&progress.label),
                                total_iterations_from_live_label(&progress.label),
                                role_from_live_label(&progress.label)
                                    .is_some_and(|role| role.contains(":sub-agent-")),
                                Some(elapsed.as_millis()),
                                None,
                                vec![],
                                format!(
                                    "[amon-hen] running {} for {}s{}",
                                    progress.label,
                                    elapsed.as_secs(),
                                    timeout_detail
                                ),
                            ),
                        );
                        next_live_tick += Duration::from_secs(10);
                    }
                }
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => {
                if let Some(handle) = stdin_writer.take() {
                    let _ = handle.join();
                }
                return CommandResult {
                    command: command.to_string(),
                    args: args.to_vec(),
                    code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    timed_out,
                    cancelled,
                    error: Some(error.to_string()),
                    timeout_ms,
                    duration_ms: started.elapsed().as_millis(),
                };
            }
        }
    }

    if let Some(handle) = stdin_writer {
        let _ = handle.join();
    }

    CommandResult {
        command: command.to_string(),
        args: args.to_vec(),
        code,
        stdout: stdout
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default(),
        stderr: stderr
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default(),
        timed_out,
        cancelled,
        error: cancelled.then(|| "cancelled".to_string()),
        timeout_ms,
        duration_ms: started.elapsed().as_millis(),
    }
}

fn read_pipe<R>(
    mut pipe: R,
    progress: Option<CommandProgress>,
    stream: &'static str,
) -> thread::JoinHandle<String>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut text = String::new();
        let mut pending = String::new();
        let mut buffer = [0u8; 4096];
        let mut last_stream_event = Instant::now() - PROVIDER_STREAM_EVENT_MIN_INTERVAL;
        let mut stream_state = ProviderStreamState::new();
        loop {
            let read = match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(_) => break,
            };
            let chunk = String::from_utf8_lossy(&buffer[..read]).to_string();
            text.push_str(&chunk);
            pending.push_str(&chunk);
            while let Some(newline) = pending.find('\n') {
                let line = pending[..newline].trim_end_matches('\r').to_string();
                pending = pending[newline + 1..].to_string();
                if stream_state.maybe_emit_assistant_delta(
                    &progress,
                    stream,
                    &line,
                    &text,
                    &mut last_stream_event,
                ) {
                    continue;
                }
                maybe_emit_provider_stream_line(
                    &progress,
                    stream,
                    &line,
                    &text,
                    &mut last_stream_event,
                );
            }
            if !chunk.contains('\n') {
                maybe_emit_provider_stream_chunk(
                    &progress,
                    stream,
                    &chunk,
                    &text,
                    &mut last_stream_event,
                );
            }
        }
        stream_state.flush(&progress, stream, &text);
        if !pending.trim().is_empty() {
            emit_provider_stream_line(&progress, stream, &pending, &text);
        }
        text
    })
}

struct ProviderStreamState {
    assistant_text: String,
    emitted_assistant_chars: usize,
}

impl ProviderStreamState {
    fn new() -> Self {
        Self {
            assistant_text: String::new(),
            emitted_assistant_chars: 0,
        }
    }

    fn maybe_emit_assistant_delta(
        &mut self,
        progress: &Option<CommandProgress>,
        stream: &str,
        line: &str,
        accumulated: &str,
        last_stream_event: &mut Instant,
    ) -> bool {
        let Some(progress) = progress else {
            return false;
        };
        let Some(provider) = provider_from_live_label(&progress.label) else {
            return false;
        };
        let Some(delta) = provider_assistant_text_delta(provider, line) else {
            return false;
        };
        if delta.is_empty() {
            return true;
        }
        self.assistant_text.push_str(&delta);
        let now = Instant::now();
        let chars_since_emit = self
            .assistant_text
            .chars()
            .count()
            .saturating_sub(self.emitted_assistant_chars);
        if chars_since_emit >= 96
            || now.duration_since(*last_stream_event) >= PROVIDER_STREAM_EVENT_MIN_INTERVAL
        {
            self.emit_snapshot(progress, stream, accumulated);
            *last_stream_event = now;
        }
        true
    }

    fn flush(&mut self, progress: &Option<CommandProgress>, stream: &str, accumulated: &str) {
        let Some(progress) = progress else {
            return;
        };
        if self.assistant_text.chars().count() > self.emitted_assistant_chars {
            self.emit_snapshot(progress, stream, accumulated);
        }
    }

    fn emit_snapshot(&mut self, progress: &CommandProgress, stream: &str, accumulated: &str) {
        let visible = format!(
            "assistant live: {}",
            sanitize_status_detail(&tail_chars(&self.assistant_text, 900))
        );
        emit_provider_stream_visible(progress, stream, accumulated, &visible, Vec::new());
        self.emitted_assistant_chars = self.assistant_text.chars().count();
    }
}

fn maybe_emit_provider_stream_chunk(
    progress: &Option<CommandProgress>,
    stream: &str,
    chunk: &str,
    accumulated: &str,
    last_stream_event: &mut Instant,
) {
    let clipped = chunk.trim();
    if clipped.len() >= 24 {
        maybe_emit_provider_stream_line(progress, stream, clipped, accumulated, last_stream_event);
    }
}

fn maybe_emit_provider_stream_line(
    progress: &Option<CommandProgress>,
    stream: &str,
    line: &str,
    accumulated: &str,
    last_stream_event: &mut Instant,
) {
    let now = Instant::now();
    let important = provider_stream_line_is_important(line);
    if important || now.duration_since(*last_stream_event) >= PROVIDER_STREAM_EVENT_MIN_INTERVAL {
        emit_provider_stream_line(progress, stream, line, accumulated);
        *last_stream_event = now;
    }
}

fn provider_stream_line_is_important(line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    [
        "tool_use",
        "tool_call",
        "functioncall",
        "function_call",
        "command_execution",
        "item.completed",
        "\"result\"",
        "\"error\"",
        "error",
        "failed",
        "permission",
    ]
    .iter()
    .any(|needle| line.contains(needle))
}

fn emit_provider_stream_line(
    progress: &Option<CommandProgress>,
    stream: &str,
    line: &str,
    accumulated: &str,
) {
    let Some(progress) = progress else {
        return;
    };
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    let provider = provider_from_live_label(&progress.label);
    let tool_calls = provider
        .map(|provider| extract_tool_usage(provider, line, ""))
        .unwrap_or_default();
    let visible = match provider_visible_stream(provider, line, &tool_calls) {
        StreamDisplay::Visible(visible) => visible,
        StreamDisplay::Suppress => return,
    };
    emit_provider_stream_visible(progress, stream, accumulated, &visible, tool_calls);
}

fn emit_provider_stream_visible(
    progress: &CommandProgress,
    stream: &str,
    accumulated: &str,
    visible: &str,
    tool_calls: Vec<ToolUsage>,
) {
    let provider = provider_from_live_label(&progress.label);
    let role = role_from_live_label(&progress.label);
    let output_tokens = estimate_tokens(accumulated);
    let token_usage = provider.map(|_| TokenUsage {
        input: progress.input_tokens,
        output: output_tokens,
        total: progress.input_tokens + output_tokens,
        estimated: true,
        source: "live-stream-estimate".to_string(),
    });
    emit_runtime_event(
        &progress.sink,
        progress.sink.is_none(),
        progress_event_with_context(
            provider,
            role,
            ProgressStage::Heartbeat,
            Some("streaming"),
            iteration_from_live_label(&progress.label),
            total_iterations_from_live_label(&progress.label),
            role.is_some_and(|role| role.contains(":sub-agent-")),
            None,
            token_usage,
            tool_calls,
            format!(
                "[amon-hen] stream {} {stream}: {}",
                progress.label,
                truncate(visible, 220)
            ),
        ),
    );
}

#[derive(Debug, Eq, PartialEq)]
enum StreamDisplay {
    Visible(String),
    Suppress,
}

fn provider_visible_stream(
    provider: Option<&str>,
    line: &str,
    tool_calls: &[ToolUsage],
) -> StreamDisplay {
    let Some(provider) = provider else {
        return StreamDisplay::Visible(sanitize_status_detail(line));
    };
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        if looks_like_json_line(line) {
            return StreamDisplay::Suppress;
        }
        return StreamDisplay::Visible(sanitize_status_detail(line));
    };
    let visible = match provider {
        "claude" => claude_visible_stream(&value),
        "gemini" => gemini_visible_stream(&value),
        "codex" => codex_visible_stream(&value),
        _ => None,
    }
    .or_else(|| tool_calls.first().map(visible_tool_summary));
    visible
        .filter(|text| !text.trim().is_empty())
        .map(StreamDisplay::Visible)
        .unwrap_or(StreamDisplay::Suppress)
}

fn provider_assistant_text_delta(provider: &str, line: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    match provider {
        "claude" => claude_assistant_text_delta(&value),
        _ => None,
    }
}

fn claude_assistant_text_delta(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) == Some("stream_event") {
        return value.get("event").and_then(claude_assistant_text_delta);
    }
    match value.get("type").and_then(Value::as_str) {
        Some("content_block_delta") => {
            let delta = value.get("delta")?;
            if delta.get("type").and_then(Value::as_str) == Some("text_delta") {
                return delta
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
        }
        Some("message_delta" | "message_start" | "message_stop" | "content_block_stop") => {}
        _ => {
            if let Some(text) = value.pointer("/delta/text").and_then(Value::as_str) {
                return Some(text.to_string());
            }
        }
    }
    None
}

fn tail_chars(text: &str, max_chars: usize) -> String {
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    let tail = text
        .chars()
        .skip(total.saturating_sub(max_chars))
        .collect::<String>();
    format!("...{tail}")
}

fn visible_tool_summary(tool: &ToolUsage) -> String {
    let detail = sanitize_status_detail(&tool.detail);
    if detail.is_empty() {
        format!("tool {}: {}", tool.status, tool.name)
    } else {
        format!("tool {}: {} {}", tool.status, tool.name, detail)
    }
}

fn claude_visible_stream(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) == Some("stream_event") {
        return value.get("event").and_then(claude_visible_stream);
    }
    match value.get("type").and_then(Value::as_str) {
        Some("content_block_delta") => return claude_visible_delta(value.get("delta")?),
        Some("content_block_start") => {
            return value
                .get("content_block")
                .and_then(visible_content_block)
                .map(|text| sanitize_status_detail(&text));
        }
        Some("content_block_stop" | "message_start" | "message_delta" | "message_stop") => {
            return None;
        }
        _ => {}
    }
    if let Some(result) = non_empty_str(value.get("result")) {
        return Some(format!("result: {}", sanitize_status_detail(result)));
    }
    if let Some(content) = value.pointer("/message/content").and_then(Value::as_array) {
        if let Some(text) = visible_content_blocks(content) {
            return Some(text);
        }
    }
    if let Some(delta) = value.pointer("/delta/text").and_then(Value::as_str) {
        return Some(format!("assistant: {}", sanitize_status_detail(delta)));
    }
    if let Some(block) = value.pointer("/content_block") {
        if let Some(text) = visible_content_block(block) {
            return Some(text);
        }
    }
    if let Some(error) = value.pointer("/error/message").and_then(Value::as_str) {
        return Some(format!("error: {}", sanitize_status_detail(error)));
    }
    None
}

fn claude_visible_delta(delta: &Value) -> Option<String> {
    match delta.get("type").and_then(Value::as_str) {
        Some("text_delta") => delta
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(|text| format!("assistant: {}", sanitize_status_detail(text))),
        Some("input_json_delta" | "thinking_delta" | "signature_delta") => None,
        _ => delta
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(|text| format!("assistant: {}", sanitize_status_detail(text))),
    }
}

fn claude_output_text(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) == Some("stream_event") {
        return value.get("event").and_then(claude_output_text);
    }
    match value.get("type").and_then(Value::as_str) {
        Some("content_block_delta") => {
            let delta = value.get("delta")?;
            return match delta.get("type").and_then(Value::as_str) {
                Some("text_delta") => non_empty_str(delta.get("text")).map(str::to_string),
                _ => None,
            };
        }
        Some("content_block_start") => {
            let block = value.get("content_block")?;
            if matches!(
                block.get("type").and_then(Value::as_str),
                Some("text" | "output_text")
            ) {
                return non_empty_str(block.get("text")).map(str::to_string);
            }
        }
        _ => {}
    }
    if let Some(result) = non_empty_str(value.get("result")) {
        return Some(result.to_string());
    }
    value
        .pointer("/message/content")
        .and_then(Value::as_array)
        .map(|content| {
            content
                .iter()
                .filter_map(|block| {
                    matches!(
                        block.get("type").and_then(Value::as_str),
                        Some("text" | "output_text")
                    )
                    .then(|| non_empty_str(block.get("text")))
                    .flatten()
                })
                .collect::<String>()
        })
        .filter(|text| !text.trim().is_empty())
}

fn gemini_visible_stream(value: &Value) -> Option<String> {
    if let Some(response) = non_empty_str(value.get("response")) {
        return Some(format!("assistant: {}", sanitize_status_detail(response)));
    }
    if let Some(text) = non_empty_str(value.get("text")) {
        return Some(format!("assistant: {}", sanitize_status_detail(text)));
    }
    if let Some(text) = value
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
    {
        return Some(format!("assistant: {}", sanitize_status_detail(text)));
    }
    if let Some(call) = value
        .pointer("/functionCall")
        .or_else(|| value.pointer("/candidates/0/content/parts/0/functionCall"))
    {
        return Some(format!("tool: {}", function_call_summary(call)));
    }
    if let Some(error) = value.pointer("/error/message").and_then(Value::as_str) {
        return Some(format!("error: {}", sanitize_status_detail(error)));
    }
    None
}

fn codex_visible_stream(value: &Value) -> Option<String> {
    if let Some(message) = non_empty_str(value.get("message")) {
        return Some(format!("assistant: {}", sanitize_status_detail(message)));
    }
    if let Some(text) = non_empty_str(value.get("text")) {
        return Some(format!("assistant: {}", sanitize_status_detail(text)));
    }
    if let Some(item) = value.get("item") {
        return codex_visible_item(item);
    }
    if let Some(error) = value.pointer("/error/message").and_then(Value::as_str) {
        return Some(format!("error: {}", sanitize_status_detail(error)));
    }
    None
}

fn codex_visible_item(item: &Value) -> Option<String> {
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
    match item_type {
        "message" => codex_message_text(item),
        "command_execution" => {
            let command = item
                .get("command")
                .and_then(Value::as_str)
                .map(sanitize_status_detail)
                .unwrap_or_else(|| "command".to_string());
            let output = item
                .get("aggregated_output")
                .or_else(|| item.get("output"))
                .and_then(Value::as_str)
                .map(sanitize_status_detail)
                .unwrap_or_default();
            if output.trim().is_empty() {
                Some(format!("tool: shell {command}"))
            } else {
                Some(format!(
                    "tool: shell {command} -> {}",
                    truncate(&output, 140)
                ))
            }
        }
        "function_call" | "tool_call" => Some(format!("tool: {}", function_call_summary(item))),
        "error" => item
            .get("message")
            .and_then(Value::as_str)
            .map(|message| format!("error: {}", sanitize_status_detail(message))),
        "reasoning" => item
            .get("summary")
            .and_then(Value::as_str)
            .filter(|summary| !summary.trim().is_empty())
            .map(|summary| format!("reasoning summary: {}", sanitize_status_detail(summary))),
        _ => None,
    }
}

fn codex_message_text(item: &Value) -> Option<String> {
    if let Some(text) = non_empty_str(item.get("text")) {
        return Some(sanitize_status_detail(text));
    }
    item.get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| visible_content_blocks(blocks))
}

fn visible_content_blocks(blocks: &[Value]) -> Option<String> {
    let parts = blocks
        .iter()
        .filter_map(visible_content_block)
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join(" | "))
}

fn visible_content_block(block: &Value) -> Option<String> {
    match block.get("type").and_then(Value::as_str) {
        Some("text") | Some("output_text") => non_empty_str(block.get("text"))
            .map(|text| format!("assistant: {}", sanitize_status_detail(text))),
        Some("tool_use") => {
            let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
            let input = block
                .get("input")
                .map(short_json_detail)
                .unwrap_or_default();
            if input.is_empty() {
                Some(format!("tool: {name}"))
            } else {
                Some(format!("tool: {name} {}", truncate(&input, 140)))
            }
        }
        Some("function_call") | Some("tool_call") => {
            Some(format!("tool: {}", function_call_summary(block)))
        }
        _ => non_empty_str(block.get("text"))
            .map(|text| format!("assistant: {}", sanitize_status_detail(text))),
    }
}

fn function_call_summary(value: &Value) -> String {
    let name = value
        .get("name")
        .or_else(|| value.get("tool"))
        .or_else(|| value.get("function"))
        .and_then(Value::as_str)
        .unwrap_or("tool");
    let args = value
        .get("args")
        .or_else(|| value.get("arguments"))
        .or_else(|| value.get("input"))
        .map(short_json_detail)
        .unwrap_or_default();
    if args.is_empty() {
        name.to_string()
    } else {
        format!("{name} {}", truncate(&args, 140))
    }
}

fn non_empty_str(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn finalize_engine(
    name: &str,
    bin: &str,
    duration_ms: u128,
    command_result: CommandResult,
    options: EngineRunOptions,
    sub_agents: Vec<EngineResult>,
) -> EngineResult {
    let output = match Engine::parse(name) {
        Some(Engine::Claude) => parse_claude_output(&command_result.stdout),
        Some(Engine::Gemini) => parse_gemini_output(&command_result.stdout),
        Some(Engine::Codex) => parse_codex_output(&command_result.stdout),
        None => command_result.stdout.trim().to_string(),
    };
    let status = if command_result.cancelled {
        "cancelled"
    } else if let Some(error) = command_result.error.as_deref() {
        spawn_failure_status(error)
    } else if command_result.timed_out {
        "timeout"
    } else if command_result.code != Some(0) || output.trim().is_empty() {
        "error"
    } else {
        "ok"
    };
    let detail = if status == "ok" {
        String::new()
    } else if command_result.cancelled {
        "Cancelled by Studio/user request.".to_string()
    } else if command_result.timed_out {
        format!("Timed out after {}s.", command_result.timeout_ms / 1000)
    } else {
        compact_failure(&command_result)
    };
    let usage = aggregate_token_usage(
        extract_token_usage(&command_result.stdout, &options.prompt, &output)
            .unwrap_or_else(|| token_usage(&options.prompt, &output)),
        &sub_agents,
    );
    let tool_calls = extract_tool_usage(name, &command_result.stdout, &command_result.stderr);
    EngineResult {
        name: name.to_string(),
        bin: Some(bin.to_string()),
        status: status.to_string(),
        duration_ms,
        detail,
        exit_code: command_result.code,
        stdout: command_result.stdout,
        stderr: command_result.stderr,
        output,
        command: format_command(&command_result.command, &command_result.args),
        token_usage: usage,
        tool_calls,
        sub_agents,
        role: options.role,
        iteration: options.iteration,
        total_iterations: options.total_iterations,
        team_size: options.team_size,
    }
}

fn compact_failure(result: &CommandResult) -> String {
    if let Some(error) = &result.error {
        return error.clone();
    }
    if !result.stderr.trim().is_empty() {
        return result.stderr.trim().to_string();
    }
    if !result.stdout.trim().is_empty() {
        return result.stdout.trim().to_string();
    }
    result
        .code
        .map(|code| format!("Exited with code {code}."))
        .unwrap_or_else(|| "Command failed.".to_string())
}

fn command_telemetry(result: &CommandResult) -> CommandTelemetry {
    let status = if result.cancelled {
        "cancelled"
    } else if let Some(error) = result.error.as_deref() {
        spawn_failure_status(error)
    } else if result.timed_out {
        "timeout"
    } else if result.code == Some(0) {
        "ok"
    } else {
        "error"
    };
    CommandTelemetry {
        command: format_command(&result.command, &result.args),
        status: status.to_string(),
        detail: truncate(&sanitize_status_detail(&compact_failure(result)), 600),
        exit_code: result.code,
        duration_ms: result.duration_ms,
        stdout_chars: result.stdout.len(),
        stderr_chars: result.stderr.len(),
        timed_out: result.timed_out,
    }
}

fn spawn_error_status(error: &io::Error) -> &'static str {
    if error.kind() == io::ErrorKind::NotFound {
        "missing"
    } else {
        "error"
    }
}

fn spawn_failure_status(error: &str) -> &'static str {
    let lowered = error.to_ascii_lowercase();
    if lowered.contains("no such file")
        || lowered.contains("not found")
        || lowered.contains("cannot find")
    {
        "missing"
    } else {
        "error"
    }
}

fn parse_codex_output(stdout: &str) -> String {
    stdout.trim().to_string()
}

fn parse_claude_output(stdout: &str) -> String {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(value) = extract_json(trimmed) {
        if let Some(result) = value.get("result").and_then(Value::as_str) {
            return result.trim().to_string();
        }
    }
    let mut latest = String::new();
    let mut streamed_text = String::new();
    for line in trimmed.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("result") {
            if let Some(result) = value.get("result").and_then(Value::as_str) {
                latest = result.trim().to_string();
            }
            continue;
        }
        if let Some(text) = claude_output_text(&value) {
            if value.get("type").and_then(Value::as_str) == Some("stream_event") {
                streamed_text.push_str(&text);
                latest = streamed_text.trim().to_string();
            } else if !text.trim().is_empty() {
                latest = text.trim().to_string();
            }
        }
    }
    if latest.is_empty() {
        trimmed.to_string()
    } else {
        latest
    }
}

fn parse_gemini_output(stdout: &str) -> String {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(value) = extract_json(trimmed) {
        if let Some(response) = value.get("response").and_then(Value::as_str) {
            return response.trim().to_string();
        }
    }
    trimmed.to_string()
}

fn extract_json(text: &str) -> Option<Value> {
    serde_json::from_str::<Value>(text).ok().or_else(|| {
        let start = text.find('{')?;
        let end = text.rfind('}')?;
        serde_json::from_str::<Value>(&text[start..=end]).ok()
    })
}

fn build_member_prompt(input: MemberPromptInput<'_>) -> String {
    let mut sections = vec![
        "You are one member of a multi-model Amon Hen run.".to_string(),
        format!(
            "Amon Hen workflow: iteration {iteration} of {}.",
            input.workflow.iterations,
            iteration = input.iteration
        ),
        input
            .workflow
            .lead
            .as_ref()
            .map(|lead| format!("Lead model: {lead}."))
            .unwrap_or_else(|| "Lead model: auto.".to_string()),
        input
            .workflow
            .planner
            .as_ref()
            .map(|planner| format!("Planner model: {planner}."))
            .unwrap_or_else(|| "Planner model: none.".to_string()),
        format!("Your assigned role: {}.", input.role),
        role_instruction(input.role).to_string(),
        "Answer the user query directly.".to_string(),
        "Do not introduce yourself.".to_string(),
        "Do not describe your tools, environment, or capabilities unless the user explicitly asks."
            .to_string(),
        "Be concise unless the user asks for depth.".to_string(),
    ];
    if input.team_size > 0 {
        sections.push(format!(
            "Team work: you may coordinate up to {} internal sub-agents or subtasks inside your own CLI if that helps.",
            input.team_size
        ));
    }
    if input.workflow.handoff {
        sections.push(
            "Handoff mode is enabled. Treat earlier Amon Hen outputs as working context."
                .to_string(),
        );
    }
    if input.workflow.planner_mode == PLANNER_MODE_REVIEW_CHAIN {
        sections.push(
            "Review-chain mode is enabled. Each Amon Hen member runs after the previous member. \
             Review prior agent output and the current repo state before changing anything, preserve accepted work, \
             call out conflicts, and only make deliberate deltas that survive that review."
                .to_string(),
        );
    }
    if !input.plan_output.trim().is_empty() {
        sections.push(format!("Planner handoff:\n{}", input.plan_output.trim()));
    }
    let context = input
        .previous_iteration
        .iter()
        .chain(input.handoff_results.iter())
        .filter(|result| result.status == "ok" && !result.output.trim().is_empty())
        .map(|result| {
            format!(
                "### {} ({})\n{}",
                result.name,
                result.role,
                result.output.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if !context.is_empty() {
        sections.push(format!("Earlier Amon Hen handoffs:\n{context}"));
    }
    sections.push(format!("Current user query:\n{}", input.query.trim()));
    sections.join("\n\n")
}

fn build_summary_prompt(
    query: &str,
    responses: &[EngineResult],
    workflow: &Workflow,
    max_member_chars: usize,
) -> String {
    let blocks = responses
        .iter()
        .map(|response| {
            let output = truncate(&response.output, max_member_chars);
            format!("### {}\n{}", response.name, output.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "You are synthesizing answers from multiple AI CLI tools.\n\nAmon Hen workflow: {} iteration{}, {}.\nLead model: {}.\nPlanner model: {}.\nProduce one final answer to the original user query.\nAnswer directly. Call out meaningful disagreement or uncertainty when it exists.\n\nCurrent user query:\n{}\n\nAmon Hen member responses:\n{}",
        workflow.iterations,
        if workflow.iterations == 1 { "" } else { "s" },
        if workflow.handoff { "handoff enabled" } else { "parallel consultation" },
        workflow.lead.as_deref().unwrap_or("auto"),
        workflow.planner.as_deref().unwrap_or("none"),
        query.trim(),
        blocks
    )
}

fn role_instruction(role: &str) -> &'static str {
    match role {
        "planner" => "Plan the work: identify the approach, risks, checkpoints, and useful handoffs for the executors.",
        "lead" => "Lead the work: make the strongest direct attempt while watching for conflicts you may need to resolve in synthesis.",
        "lead+planner" => "Plan and lead the work: produce a practical plan, then make the strongest direct attempt from that plan.",
        _ => "Execute the work: use any plan or handoff context, then produce your independent best answer.",
    }
}

fn execution_order(members: &[String], planner: Option<&str>, lead: Option<&str>) -> Vec<String> {
    let mut ordered = Vec::new();
    if let Some(planner) = planner {
        if members.iter().any(|member| member == planner) {
            ordered.push(planner.to_string());
        }
    }
    ordered.extend(
        members
            .iter()
            .filter(|member| Some(member.as_str()) != planner && Some(member.as_str()) != lead)
            .cloned(),
    );
    if lead != planner {
        if let Some(lead) = lead {
            if members.iter().any(|member| member == lead) {
                ordered.push(lead.to_string());
            }
        }
    }
    ordered
}

fn role_for(member: &str, workflow: &Workflow) -> String {
    let is_lead = workflow.lead.as_deref() == Some(member);
    let is_planner = workflow.planner.as_deref() == Some(member);
    match (is_lead, is_planner) {
        (true, true) => "lead+planner",
        (true, false) => "lead",
        (false, true) => "planner",
        (false, false) => "executor",
    }
    .to_string()
}

fn pick_summarizer(resolved: &ResolvedArgs, successes: &[EngineResult]) -> String {
    if resolved.raw.summarizer != "auto" {
        return resolved.raw.summarizer.clone();
    }
    if let Some(lead) = &resolved.raw.lead {
        if successes.iter().any(|result| &result.name == lead) {
            return lead.clone();
        }
    }
    DEFAULT_SUMMARIZER_ORDER
        .iter()
        .find(|name| successes.iter().any(|result| result.name == **name))
        .unwrap_or(&successes[0].name.as_str())
        .to_string()
}

fn resolve_binary(name: &str) -> String {
    let Some(engine) = Engine::parse(name) else {
        return name.to_string();
    };
    std::env::var(engine.binary_env_var())
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| engine.as_str().to_string())
}

fn token_usage(prompt: &str, output: &str) -> TokenUsage {
    let input = estimate_tokens(prompt);
    let output = estimate_tokens(output);
    TokenUsage {
        input,
        output,
        total: input + output,
        estimated: true,
        source: "estimate".to_string(),
    }
}

fn aggregate_token_usage(mut usage: TokenUsage, sub_agents: &[EngineResult]) -> TokenUsage {
    if sub_agents.is_empty() {
        return usage;
    }
    for agent in sub_agents {
        usage.input += agent.token_usage.input;
        usage.output += agent.token_usage.output;
        usage.total += agent.token_usage.total;
        usage.estimated |= agent.token_usage.estimated;
    }
    usage.source = format!("{}+sub-agents", usage.source);
    usage
}

fn aggregate_results_token_usage<'a>(
    results: impl IntoIterator<Item = &'a EngineResult>,
) -> TokenUsage {
    let mut usage = TokenUsage {
        input: 0,
        output: 0,
        total: 0,
        estimated: false,
        source: "aggregate".to_string(),
    };
    for result in results {
        usage.input += result.token_usage.input;
        usage.output += result.token_usage.output;
        usage.total += result.token_usage.total;
        usage.estimated |= result.token_usage.estimated;
    }
    usage
}

fn extract_token_usage(stdout: &str, prompt: &str, output: &str) -> Option<TokenUsage> {
    let mut usage = TokenAccumulator::default();
    for value in json_values(stdout) {
        collect_token_counts(&value, &mut usage);
    }
    if usage.input.is_none() && usage.output.is_none() && usage.total.is_none() {
        return None;
    }
    let input = usage.input.unwrap_or_else(|| estimate_tokens(prompt));
    let output = usage.output.unwrap_or_else(|| estimate_tokens(output));
    let total = usage.total.unwrap_or(input + output);
    Some(TokenUsage {
        input,
        output,
        total,
        estimated: usage.input.is_none() || usage.output.is_none(),
        source: "provider".to_string(),
    })
}

#[derive(Default)]
struct TokenAccumulator {
    input: Option<usize>,
    output: Option<usize>,
    total: Option<usize>,
}

fn collect_token_counts(value: &Value, usage: &mut TokenAccumulator) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let normalized = key.replace(['_', '-'], "").to_ascii_lowercase();
                if let Some(count) = value.as_u64().map(|value| value as usize) {
                    match normalized.as_str() {
                        "inputtokens" | "inputtokencount" | "prompttokens" | "prompttokencount" => {
                            usage.input = Some(count)
                        }
                        "outputtokens"
                        | "outputtokencount"
                        | "completiontokens"
                        | "candidatestokencount" => usage.output = Some(count),
                        "totaltokens" | "totaltokencount" => usage.total = Some(count),
                        _ => {}
                    }
                }
                collect_token_counts(value, usage);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_token_counts(value, usage);
            }
        }
        _ => {}
    }
}

fn extract_tool_usage(provider: &str, stdout: &str, stderr: &str) -> Vec<ToolUsage> {
    let mut tools = Vec::new();
    for value in json_values(stdout) {
        collect_tool_usage(&value, &mut tools);
    }
    for line in stdout.lines().chain(stderr.lines()) {
        if let Some(tool) = line_tool_usage(provider, line) {
            tools.push(tool);
        }
    }
    dedupe_tools(tools)
}

fn collect_tool_usage(value: &Value, tools: &mut Vec<ToolUsage>) {
    match value {
        Value::Object(map) => {
            let type_name = map.get("type").and_then(Value::as_str).unwrap_or_default();
            let name = map
                .get("name")
                .or_else(|| map.get("tool"))
                .or_else(|| map.get("tool_name"))
                .or_else(|| map.get("toolName"))
                .and_then(Value::as_str);
            if let Some(name) = name.filter(|_| {
                type_name.contains("tool")
                    || map.contains_key("input")
                    || map.contains_key("arguments")
                    || map.contains_key("toolCall")
            }) {
                tools.push(ToolUsage {
                    name: name.to_string(),
                    kind: if type_name.is_empty() {
                        "tool"
                    } else {
                        type_name
                    }
                    .to_string(),
                    status: "observed".to_string(),
                    detail: short_json_detail(value),
                });
            }
            for value in map.values() {
                collect_tool_usage(value, tools);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_tool_usage(value, tools);
            }
        }
        _ => {}
    }
}

fn line_tool_usage(provider: &str, line: &str) -> Option<ToolUsage> {
    let trimmed = line.trim();
    if looks_like_json_line(trimmed) {
        return None;
    }
    let lowered = trimmed.to_ascii_lowercase();
    let marker = ["running shell:", "tool:", "tool_use", "mcp:", "command:"]
        .iter()
        .find(|marker| lowered.contains(**marker))?;
    Some(ToolUsage {
        name: marker.trim_end_matches(':').to_string(),
        kind: provider.to_string(),
        status: "observed".to_string(),
        detail: truncate(trimmed, 240),
    })
}

fn short_json_detail(value: &Value) -> String {
    serde_json::to_string(value)
        .map(|text| truncate(&text, 240))
        .unwrap_or_default()
}

fn dedupe_tools(tools: Vec<ToolUsage>) -> Vec<ToolUsage> {
    let mut seen = HashSet::new();
    tools
        .into_iter()
        .filter(|tool| seen.insert(format!("{}:{}:{}", tool.kind, tool.name, tool.detail)))
        .collect()
}

fn json_values(text: &str) -> Vec<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return vec![];
    }
    let mut values = Vec::new();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        values.push(value);
        return values;
    }
    for line in trimmed.lines() {
        if let Ok(value) = serde_json::from_str::<Value>(line.trim()) {
            values.push(value);
        }
    }
    values
}

fn looks_like_json_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn estimate_tokens(text: &str) -> usize {
    if text.trim().is_empty() {
        0
    } else {
        text.len().div_ceil(TOKEN_ESTIMATE_CHARS_PER_TOKEN)
    }
}

fn format_command(command: &str, args: &[String]) -> String {
    std::iter::once(command.to_string())
        .chain(args.iter().map(|arg| shell_quote(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "_./:=@%+-".contains(ch))
    {
        value.to_string()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let mut truncated = text.chars().take(max_chars).collect::<String>();
        truncated.push_str("\n...[truncated]");
        truncated
    }
}

fn should_show_banner(raw: &CliArgs) -> bool {
    !raw.no_banner && !raw.headless && !raw.json && !raw.json_stream && !raw.plain
}

fn linear_delivery_requested(raw: &CliArgs) -> bool {
    raw.deliver_linear
        || raw.linear_until_complete
        || raw.linear_watch
        || !raw.linear_issue.is_empty()
        || raw
            .linear_query
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
        || !raw.linear_project.is_empty()
        || !raw.linear_epic.is_empty()
}

fn render_banner() -> &'static str {
    "    _                         _   _            \n   / \\   _ __ ___   ___  _ __ | | | | ___ _ __  \n  / _ \\ | '_ ` _ \\ / _ \\| '_ \\| |_| |/ _ \\ '_ \\ \n / ___ \\| | | | | | (_) | | | |  _  |  __/ | | |\n/_/   \\_\\_| |_| |_|\\___/|_| |_|_| |_|\\___|_| |_|"
}

fn render_human_result(result: &AmonHenResult, verbose: bool) -> String {
    let mut lines = Vec::new();
    if verbose {
        lines.push(format!(
            "Amon Hen consulted: {}",
            result.members_requested.join(", ")
        ));
        for command in &result.prompt_commands {
            lines.push(format!(
                "cmd [{}] {} ({:.1}s)",
                command.status,
                command.command,
                command.duration_ms as f64 / 1000.0
            ));
        }
        for (index, member) in result.members.iter().enumerate() {
            lines.push(format!(
                "{}. [{}] {} ({:.1}s, tokens:{}, tools:{}, sub-agents:{}){}",
                index + 1,
                member.status,
                member.name,
                member.duration_ms as f64 / 1000.0,
                member.token_usage.total,
                member.tool_calls.len(),
                member.sub_agents.len(),
                if member.detail.is_empty() { "" } else { ": " }
            ));
            if !member.detail.is_empty() {
                lines.push(format!("   {}", member.detail));
            }
            for sub_agent in &member.sub_agents {
                lines.push(format!(
                    "   - {} [{}] tokens:{} tools:{}",
                    sub_agent.role,
                    sub_agent.status,
                    sub_agent.token_usage.total,
                    sub_agent.tool_calls.len()
                ));
            }
            if member.status == "ok" {
                lines.push(indent(&member.output, "   "));
            }
        }
        lines.push("----------- synthesis -----------".to_string());
    }
    if result.summary.status == "ok" {
        lines.push(result.summary.output.trim().to_string());
    } else {
        lines.push(format!("Synthesis failed: {}", result.summary.detail));
    }
    lines.join("\n")
}

fn indent(text: &str, prefix: &str) -> String {
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_success(result: &AmonHenResult) -> bool {
    result.members.iter().any(|member| member.status == "ok") && result.summary.status == "ok"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as TestMutex;

    static ENV_LOCK: TestMutex<()> = TestMutex::new(());

    struct ProviderEnvGuard {
        saved: Vec<(&'static str, Option<OsString>)>,
    }

    impl ProviderEnvGuard {
        fn install_echo_bins() -> Option<Self> {
            let echo = Path::new("/bin/echo");
            if !echo.is_file() {
                return None;
            }
            let vars = [
                "AMON_HEN_CODEX_BIN",
                "AMON_HEN_CLAUDE_BIN",
                "AMON_HEN_GEMINI_BIN",
            ];
            let saved = vars
                .into_iter()
                .map(|name| (name, std::env::var_os(name)))
                .collect::<Vec<_>>();
            for (name, _) in &saved {
                unsafe {
                    std::env::set_var(name, echo);
                }
            }
            Some(Self { saved })
        }
    }

    impl Drop for ProviderEnvGuard {
        fn drop(&mut self) {
            for (name, value) in &self.saved {
                unsafe {
                    if let Some(value) = value {
                        std::env::set_var(name, value);
                    } else {
                        std::env::remove_var(name);
                    }
                }
            }
        }
    }

    #[test]
    fn help_flags_use_success_exit_code() {
        let long_help = CliArgs::try_parse_from(["amon-hen", "--help"]).unwrap_err();
        let short_help = CliArgs::try_parse_from(["amon-hen", "-h"]).unwrap_err();

        assert_eq!(long_help.kind(), ErrorKind::DisplayHelp);
        assert_eq!(short_help.kind(), ErrorKind::DisplayHelp);
        assert_eq!(parse_error_exit_code(long_help.kind()), 0);
        assert_eq!(parse_error_exit_code(short_help.kind()), 0);
    }

    #[test]
    fn rendered_help_is_grouped_and_compact() {
        let help = render_cli_help();

        assert!(help.contains("Core run:"));
        assert!(help.contains("Models, effort, and permissions:"));
        assert!(help.contains("Auth and provider capabilities:"));
        assert!(help.contains("Linear delivery:"));
        assert!(help.contains("--members codex,claude,gemini"));
        assert!(help.contains("--claude-permission-mode MODE"));
        assert!(help.contains("--gemini-approval-mode MODE"));
        assert!(help.contains("blocking|parallel|review-chain"));
        assert!(!help.contains("\n\n\n"));
    }

    #[test]
    fn parses_members_and_provider_flags() {
        let args = CliArgs::try_parse_from([
            "amon-hen",
            "--members",
            "codex,claude",
            "--planner",
            "codex",
            "--planner-mode",
            "parallel",
            "--handoff",
            "--lead",
            "claude",
            "--codex-effort",
            "xhigh",
            "--claude-capabilities",
            "override",
            "--claude-allowed-tools",
            "Read,Bash(git:*)",
            "--claude-tools",
            "Read,Edit",
            "--claude-agent",
            "reviewer",
            "--gemini-allowed-mcp-servers",
            "linear,github",
            "ship it",
        ])
        .unwrap();
        let resolved = resolve_args(args).unwrap();
        assert_eq!(resolved.members, vec!["codex", "claude"]);
        assert_eq!(resolved.raw.planner_mode, PLANNER_MODE_PARALLEL);
        assert_eq!(resolved.raw.codex_effort.as_deref(), Some("xhigh"));
        assert_eq!(
            resolved.raw.claude_allowed_tools,
            vec!["Read", "Bash(git:*)"]
        );
        assert_eq!(resolved.raw.claude_tools, vec!["Read", "Edit"]);
        assert_eq!(resolved.raw.claude_agent.as_deref(), Some("reviewer"));
        assert_eq!(
            resolved.raw.gemini_allowed_mcp_servers,
            vec!["linear", "github"]
        );
    }

    #[test]
    fn planner_parallel_mode_keeps_executors_unblocked() {
        let args = CliArgs::try_parse_from([
            "amon-hen",
            "--members",
            "codex,claude,gemini",
            "--planner",
            "claude",
            "--planner-mode",
            "parallel",
            "--handoff",
            "--lead",
            "claude",
            "ship it",
        ])
        .unwrap();
        let resolved = resolve_args(args).unwrap();
        let workflow = build_workflow(&resolved);
        let ordered = execution_order(
            &resolved.members,
            workflow.planner.as_deref(),
            workflow.lead.as_deref(),
        );
        let planner = workflow
            .planner
            .as_ref()
            .filter(|planner| ordered.contains(planner))
            .cloned();
        let blocking_planner = workflow.planner_mode == PLANNER_MODE_BLOCKING;
        let executors = ordered
            .into_iter()
            .filter(|member| !(blocking_planner && Some(member) == planner.as_ref()))
            .collect::<Vec<_>>();

        assert_eq!(workflow.planner_mode, PLANNER_MODE_PARALLEL);
        assert!(workflow.handoff);
        assert_eq!(executors, vec!["claude", "codex", "gemini"]);
        assert_eq!(role_for("claude", &workflow), "lead+planner");
        assert_eq!(role_for("codex", &workflow), "executor");
        assert_eq!(role_for("gemini", &workflow), "executor");
    }

    #[test]
    fn planner_review_chain_mode_serializes_agent_review() {
        let args = CliArgs::try_parse_from([
            "amon-hen",
            "--members",
            "codex,claude,gemini",
            "--planner",
            "claude",
            "--planner-mode",
            "review-chain",
            "--lead",
            "claude",
            "ship it",
        ])
        .unwrap();
        let resolved = resolve_args(args).unwrap();
        let workflow = build_workflow(&resolved);
        let ordered = execution_order(
            &resolved.members,
            workflow.planner.as_deref(),
            workflow.lead.as_deref(),
        );
        let prior = vec![EngineResult {
            name: "codex".to_string(),
            bin: Some("codex".to_string()),
            status: "ok".to_string(),
            duration_ms: 1,
            detail: String::new(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            output: "codex patch notes".to_string(),
            command: "codex".to_string(),
            token_usage: token_usage("a", "b"),
            tool_calls: vec![],
            sub_agents: vec![],
            role: "executor".to_string(),
            iteration: 1,
            total_iterations: 1,
            team_size: 0,
        }];
        let prompt = build_member_prompt(MemberPromptInput {
            query: "ship it",
            role: "executor",
            workflow: &workflow,
            iteration: 1,
            team_size: 0,
            previous_iteration: &[],
            handoff_results: &prior,
            plan_output: "",
        });

        assert_eq!(workflow.planner_mode, PLANNER_MODE_REVIEW_CHAIN);
        assert!(
            workflow.handoff,
            "review-chain should imply handoff context"
        );
        assert!(planner_mode_runs_serial(&workflow));
        assert_eq!(ordered, vec!["claude", "codex", "gemini"]);
        assert!(prompt.contains("Review-chain mode is enabled"));
        assert!(prompt.contains("### codex (executor)"));
        assert!(prompt.contains("codex patch notes"));
    }

    #[test]
    fn parses_historical_linear_flags_in_rust() {
        let args = CliArgs::try_parse_from([
            "amon-hen",
            "--banner",
            "--auth-open-browser",
            "--members",
            "codex,claude,gemini",
            "--linear-project",
            "ENG",
            "--linear-endpoint",
            "https://linear.example/graphql",
            "--linear-auth",
            "oauth",
            "--linear-max-polls",
            "2",
            "--linear-max-concurrency",
            "3",
            "--linear-workflow-file",
            "docs/linear-workflow.md",
            "--linear-workspace-strategy",
            "copy",
            "--linear-completion-gate",
            "review-or-ci",
            "--delivery-phases",
            "plan,implement,verify,ship",
            "deliver it",
        ])
        .unwrap();
        let resolved = resolve_args(args).unwrap();

        assert!(linear_delivery_requested(&resolved.raw));
        assert_eq!(
            resolved.raw.linear_endpoint.as_deref(),
            Some("https://linear.example/graphql")
        );
        assert_eq!(resolved.raw.linear_auth, "oauth");
        assert_eq!(resolved.raw.linear_max_polls, Some(2));
        assert_eq!(resolved.raw.linear_max_concurrency, 3);
        assert_eq!(
            resolved.raw.linear_workflow_file.as_deref(),
            Some(Path::new("docs/linear-workflow.md"))
        );
        assert_eq!(resolved.raw.linear_workspace_strategy, "copy");
        assert_eq!(resolved.raw.linear_completion_gate, "review-or-ci");
        assert_eq!(
            resolved.raw.delivery_phases,
            vec!["plan", "implement", "verify", "ship"]
        );
    }

    #[test]
    fn cli_role_flag_matrix_reaches_provider_options_and_prompts() {
        let args = CliArgs::try_parse_from([
            "amon-hen",
            "--members",
            "codex,claude,gemini",
            "--planner",
            "codex",
            "--lead",
            "claude",
            "--handoff",
            "--iterations",
            "3",
            "--team-work",
            "2",
            "--codex-sub-agents",
            "4",
            "--claude-sub-agents",
            "1",
            "--gemini-sub-agents",
            "0",
            "--codex-model",
            "gpt-5.2",
            "--codex-effort",
            "xhigh",
            "--codex-sandbox",
            "danger-full-access",
            "--codex-config",
            "sandbox_workspace_write.network_access=true",
            "--codex-mcp-profile",
            "repo",
            "--claude-model",
            "sonnet",
            "--claude-effort",
            "max",
            "--claude-permission-mode",
            "bypassPermissions",
            "--claude-allowed-tools",
            "Read,Edit",
            "--claude-tools",
            "Bash",
            "--claude-agent",
            "lead-reviewer",
            "--gemini-model",
            "gemini-2.5-pro",
            "--gemini-effort",
            "high",
            "--gemini-approval-mode",
            "auto_edit",
            "--gemini-settings",
            "gemini-settings.json",
            "--gemini-tools-profile",
            "repo,ci",
            "--gemini-allowed-mcp-servers",
            "linear",
            "Fix the regression",
        ])
        .unwrap();
        let resolved = resolve_args(args).unwrap();
        let workflow = build_workflow(&resolved);
        let previous = vec![EngineResult {
            name: "gemini".to_string(),
            bin: Some("gemini".to_string()),
            status: "ok".to_string(),
            duration_ms: 1,
            detail: String::new(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            output: "previous iteration context".to_string(),
            command: "gemini".to_string(),
            token_usage: token_usage("a", "b"),
            tool_calls: vec![],
            sub_agents: vec![],
            role: "executor".to_string(),
            iteration: 1,
            total_iterations: 3,
            team_size: 0,
        }];
        let handoff = vec![EngineResult {
            name: "codex".to_string(),
            bin: Some("codex".to_string()),
            status: "ok".to_string(),
            duration_ms: 1,
            detail: String::new(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            output: "planner handoff context".to_string(),
            command: "codex".to_string(),
            token_usage: token_usage("a", "b"),
            tool_calls: vec![],
            sub_agents: vec![],
            role: "planner".to_string(),
            iteration: 2,
            total_iterations: 3,
            team_size: 4,
        }];

        assert_eq!(workflow.iterations, 3);
        assert!(workflow.handoff);
        assert_eq!(workflow.team_work, 2);
        assert_eq!(workflow.teams.get("codex"), Some(&4));
        assert_eq!(workflow.teams.get("claude"), Some(&1));
        assert_eq!(workflow.teams.get("gemini"), Some(&0));
        assert_eq!(role_for("codex", &workflow), "planner");
        assert_eq!(role_for("claude", &workflow), "lead");
        assert_eq!(role_for("gemini", &workflow), "executor");

        let codex_prompt = build_member_prompt(MemberPromptInput {
            query: "Fix the regression",
            role: "planner",
            workflow: &workflow,
            iteration: 2,
            team_size: effective_team_size(&workflow, "codex"),
            previous_iteration: &previous,
            handoff_results: &handoff,
            plan_output: "plan output",
        });
        assert!(codex_prompt.contains("Amon Hen workflow: iteration 2 of 3."));
        assert!(codex_prompt.contains("Lead model: claude."));
        assert!(codex_prompt.contains("Planner model: codex."));
        assert!(codex_prompt.contains("Your assigned role: planner."));
        assert!(codex_prompt.contains("Handoff mode is enabled."));
        assert!(codex_prompt.contains("Team work: you may coordinate up to 4 internal sub-agents"));
        assert!(codex_prompt.contains("Planner handoff:\nplan output"));
        assert!(codex_prompt.contains("previous iteration context"));
        assert!(codex_prompt.contains("planner handoff context"));

        let codex_options = engine_options(
            &resolved,
            EngineOptionsInput {
                member: "codex",
                prompt: codex_prompt,
                role: "planner",
                iteration: 2,
                workflow: &workflow,
                progress: None,
                cancel: None,
            },
        );
        assert_eq!(codex_options.model.as_deref(), Some("gpt-5.2"));
        assert_eq!(codex_options.effort.as_deref(), Some("xhigh"));
        assert_eq!(
            codex_options.permission.as_deref(),
            Some("danger-full-access")
        );
        assert_eq!(codex_options.role, "planner");
        assert_eq!(codex_options.iteration, 2);
        assert_eq!(codex_options.total_iterations, 3);
        assert_eq!(codex_options.team_size, 4);
        assert_eq!(codex_options.capability.mode, CAPABILITY_OVERRIDE);
        assert_eq!(
            codex_options.capability.config,
            vec!["sandbox_workspace_write.network_access=true"]
        );
        assert_eq!(
            codex_options.capability.mcp_profile.as_deref(),
            Some("repo")
        );

        let claude_options = engine_options(
            &resolved,
            EngineOptionsInput {
                member: "claude",
                prompt: build_member_prompt(MemberPromptInput {
                    query: "Fix the regression",
                    role: "lead",
                    workflow: &workflow,
                    iteration: 2,
                    team_size: effective_team_size(&workflow, "claude"),
                    previous_iteration: &[],
                    handoff_results: &[],
                    plan_output: "",
                }),
                role: "lead",
                iteration: 2,
                workflow: &workflow,
                progress: None,
                cancel: None,
            },
        );
        assert_eq!(claude_options.model.as_deref(), Some("sonnet"));
        assert_eq!(claude_options.effort.as_deref(), Some("max"));
        assert_eq!(
            claude_options.permission.as_deref(),
            Some("bypassPermissions")
        );
        assert_eq!(claude_options.role, "lead");
        assert_eq!(claude_options.team_size, 1);
        assert_eq!(claude_options.capability.mode, CAPABILITY_OVERRIDE);
        assert_eq!(
            claude_options.capability.allowed_tools,
            vec!["Read", "Edit"]
        );
        assert_eq!(claude_options.capability.tools, vec!["Bash"]);
        assert_eq!(
            claude_options.capability.agent.as_deref(),
            Some("lead-reviewer")
        );

        let gemini_options = engine_options(
            &resolved,
            EngineOptionsInput {
                member: "gemini",
                prompt: build_member_prompt(MemberPromptInput {
                    query: "Fix the regression",
                    role: "executor",
                    workflow: &workflow,
                    iteration: 2,
                    team_size: effective_team_size(&workflow, "gemini"),
                    previous_iteration: &[],
                    handoff_results: &[],
                    plan_output: "",
                }),
                role: "executor",
                iteration: 2,
                workflow: &workflow,
                progress: None,
                cancel: None,
            },
        );
        assert_eq!(gemini_options.model.as_deref(), Some("gemini-2.5-pro"));
        assert_eq!(gemini_options.effort.as_deref(), Some("high"));
        assert_eq!(gemini_options.permission.as_deref(), Some("auto_edit"));
        assert_eq!(gemini_options.role, "executor");
        assert_eq!(gemini_options.team_size, 0);
        assert_eq!(gemini_options.capability.mode, CAPABILITY_OVERRIDE);
        assert_eq!(
            gemini_options.capability.settings.as_deref(),
            Some("gemini-settings.json")
        );
        assert_eq!(gemini_options.capability.tools_profile, vec!["repo", "ci"]);
        assert_eq!(
            gemini_options.capability.allowed_mcp_servers,
            vec!["linear"]
        );
    }

    #[test]
    fn provider_command_args_include_role_matrix_settings() {
        let cwd = std::env::current_dir().unwrap();
        let codex_options = EngineRunOptions {
            prompt: "codex prompt".to_string(),
            cwd: cwd.clone(),
            timeout_ms: 1,
            effort: Some("high".to_string()),
            model: Some("gpt-5.2".to_string()),
            permission: Some("workspace-write".to_string()),
            auth: DEFAULT_AUTH_MODE.to_string(),
            capability: ProviderCapability {
                mode: CAPABILITY_OVERRIDE.to_string(),
                config: vec!["tools.web_search=true".to_string()],
                mcp_profile: Some("repo".to_string()),
                mcp_config: vec![],
                allowed_tools: vec![],
                disallowed_tools: vec![],
                tools: vec![],
                agent: None,
                agents_json: None,
                plugin_dirs: vec![],
                strict_mcp_config: false,
                disable_slash_commands: false,
                settings: None,
                tools_profile: vec![],
                allowed_mcp_servers: vec![],
                policy: vec![],
                admin_policy: vec![],
            },
            role: "planner".to_string(),
            iteration: 2,
            total_iterations: 3,
            team_size: 4,
            is_sub_agent: false,
            live: false,
            progress: None,
            cancel: None,
        };
        let codex = run_codex("definitely-missing-codex-test-bin", &codex_options);
        assert_eq!(codex.args[0], "exec");
        assert!(codex
            .args
            .windows(2)
            .any(|pair| pair == ["--model", "gpt-5.2"]));
        assert!(codex
            .args
            .windows(2)
            .any(|pair| pair == ["-c", "model_reasoning_effort=high"]));
        assert!(codex
            .args
            .windows(2)
            .any(|pair| pair == ["-c", "tools.web_search=true"]));
        assert!(codex
            .args
            .windows(2)
            .any(|pair| pair == ["--profile", "repo"]));
        assert!(codex
            .args
            .windows(2)
            .any(|pair| pair == ["--sandbox", "workspace-write"]));

        let claude_options = EngineRunOptions {
            prompt: "claude prompt".to_string(),
            cwd: cwd.clone(),
            timeout_ms: 1,
            effort: Some("max".to_string()),
            model: Some("sonnet".to_string()),
            permission: Some("bypassPermissions".to_string()),
            auth: "social-login".to_string(),
            capability: ProviderCapability {
                mode: CAPABILITY_OVERRIDE.to_string(),
                config: vec![],
                mcp_profile: None,
                mcp_config: vec!["mcp.json".to_string()],
                allowed_tools: vec!["Read".to_string(), "Edit".to_string()],
                disallowed_tools: vec!["WebFetch".to_string()],
                tools: vec!["Bash".to_string()],
                agent: Some("lead-reviewer".to_string()),
                agents_json: Some("agents.json".to_string()),
                plugin_dirs: vec!["plugins/a".to_string(), "plugins/b".to_string()],
                strict_mcp_config: true,
                disable_slash_commands: true,
                settings: None,
                tools_profile: vec![],
                allowed_mcp_servers: vec![],
                policy: vec![],
                admin_policy: vec![],
            },
            role: "lead".to_string(),
            iteration: 2,
            total_iterations: 3,
            team_size: 1,
            is_sub_agent: false,
            live: false,
            progress: None,
            cancel: None,
        };
        let claude = run_claude("definitely-missing-claude-test-bin", &claude_options);
        assert!(!claude.args.iter().any(|arg| arg == "--bare"));
        assert!(claude
            .args
            .windows(2)
            .any(|pair| pair == ["--permission-mode", "bypassPermissions"]));
        assert!(claude
            .args
            .windows(2)
            .any(|pair| pair == ["--model", "sonnet"]));
        assert!(claude
            .args
            .windows(2)
            .any(|pair| pair == ["--effort", "max"]));
        assert!(claude
            .args
            .windows(3)
            .any(|triple| triple == ["--allowedTools", "Read", "Edit"]));
        assert!(claude
            .args
            .windows(2)
            .any(|pair| pair == ["--agent", "lead-reviewer"]));
        assert!(claude.args.iter().any(|arg| arg == "--strict-mcp-config"));
        assert!(claude
            .args
            .iter()
            .any(|arg| arg == "--disable-slash-commands"));

        let gemini_options = EngineRunOptions {
            prompt: "gemini prompt".to_string(),
            cwd,
            timeout_ms: 1,
            effort: Some("high".to_string()),
            model: Some("gemini-2.5-pro".to_string()),
            permission: Some("auto_edit".to_string()),
            auth: DEFAULT_AUTH_MODE.to_string(),
            capability: ProviderCapability {
                mode: CAPABILITY_OVERRIDE.to_string(),
                config: vec![],
                mcp_profile: None,
                mcp_config: vec![],
                allowed_tools: vec![],
                disallowed_tools: vec![],
                tools: vec![],
                agent: None,
                agents_json: None,
                plugin_dirs: vec![],
                strict_mcp_config: false,
                disable_slash_commands: false,
                settings: None,
                tools_profile: vec!["repo".to_string(), "ci".to_string()],
                allowed_mcp_servers: vec!["linear".to_string()],
                policy: vec!["default".to_string()],
                admin_policy: vec!["locked".to_string()],
            },
            role: "executor".to_string(),
            iteration: 2,
            total_iterations: 3,
            team_size: 0,
            is_sub_agent: false,
            live: false,
            progress: None,
            cancel: None,
        };
        let gemini = run_gemini("definitely-missing-gemini-test-bin", &gemini_options);
        assert!(gemini
            .args
            .windows(2)
            .any(|pair| pair == ["--model", "gemini-2.5-pro"]));
        assert!(gemini
            .args
            .windows(3)
            .any(|triple| triple == ["--extensions", "repo", "ci"]));
        assert!(gemini
            .args
            .windows(2)
            .any(|pair| pair == ["--allowed-mcp-server-names", "linear"]));
        assert!(gemini
            .args
            .windows(2)
            .any(|pair| pair == ["--policy", "default"]));
        assert!(gemini
            .args
            .windows(2)
            .any(|pair| pair == ["--admin-policy", "locked"]));
        assert!(gemini.args.windows(2).any(|pair| pair == ["-p", ""]));
        assert!(gemini
            .args
            .windows(2)
            .any(|pair| pair == ["--approval-mode", "auto_edit"]));
    }

    #[test]
    #[cfg(unix)]
    fn gemini_prompt_is_sent_on_stdin_to_avoid_arg_limit() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let fake_gemini = temp.path().join("gemini-fake");
        fs::write(
            &fake_gemini,
            "#!/bin/sh\nprintf 'ARGS:'\nfor arg in \"$@\"; do printf '<%s>' \"$arg\"; done\nprintf '\\nSTDIN:'\ncat\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&fake_gemini).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_gemini, permissions).unwrap();

        let prompt = "x".repeat(300_000);
        let options = EngineRunOptions {
            prompt: prompt.clone(),
            cwd: std::env::current_dir().unwrap(),
            timeout_ms: 5_000,
            effort: None,
            model: Some("gemini-test".to_string()),
            permission: None,
            auth: DEFAULT_AUTH_MODE.to_string(),
            capability: ProviderCapability {
                mode: CAPABILITY_INHERIT.to_string(),
                config: vec![],
                mcp_profile: None,
                mcp_config: vec![],
                allowed_tools: vec![],
                disallowed_tools: vec![],
                tools: vec![],
                agent: None,
                agents_json: None,
                plugin_dirs: vec![],
                strict_mcp_config: false,
                disable_slash_commands: false,
                settings: None,
                tools_profile: vec![],
                allowed_mcp_servers: vec![],
                policy: vec![],
                admin_policy: vec![],
            },
            role: "executor".to_string(),
            iteration: 1,
            total_iterations: 1,
            team_size: 0,
            is_sub_agent: false,
            live: false,
            progress: None,
            cancel: None,
        };
        let result = run_gemini(fake_gemini.to_str().unwrap(), &options);

        assert_eq!(result.code, Some(0));
        assert!(result.args.iter().any(|arg| arg == "-p"));
        assert!(!result.args.iter().any(|arg| arg == &prompt));
        assert!(result.stdout.contains("ARGS:<--model><gemini-test><-p><>"));
        assert!(result.stdout.contains("<--approval-mode><plan>"));
        assert!(result.stdout.ends_with(&prompt));
    }

    #[test]
    fn spawn_failure_status_distinguishes_missing_from_other_spawn_errors() {
        assert_eq!(spawn_failure_status("No such file or directory"), "missing");
        assert_eq!(
            spawn_failure_status("Argument list too long (os error 7)"),
            "error"
        );
    }

    #[test]
    fn builds_member_prompt_with_roles_and_handoff() {
        let workflow = Workflow {
            handoff: true,
            lead: Some("claude".to_string()),
            planner: Some("codex".to_string()),
            planner_mode: PLANNER_MODE_BLOCKING.to_string(),
            iterations: 2,
            team_work: 1,
            teams: HashMap::new(),
        };
        let prompt = build_member_prompt(MemberPromptInput {
            query: "Fix the bug",
            role: "planner",
            workflow: &workflow,
            iteration: 1,
            team_size: 1,
            previous_iteration: &[],
            handoff_results: &[],
            plan_output: "",
        });
        assert!(prompt.contains("Amon Hen workflow: iteration 1 of 2."));
        assert!(prompt.contains("Lead model: claude."));
        assert!(prompt.contains("Planner model: codex."));
        assert!(prompt.contains("Your assigned role: planner."));
        assert!(prompt.contains("Current user query:"));
    }

    #[test]
    fn parses_claude_stream_json_result() {
        let text = r#"{"type":"system","subtype":"status","status":"requesting"}
{"type":"result","result":"done"}"#;
        assert_eq!(parse_claude_output(text), "done");
    }

    #[test]
    fn parses_gemini_json_response() {
        assert_eq!(parse_gemini_output(r#"{"response":"hello"}"#), "hello");
    }

    #[test]
    fn estimates_tokens_with_ceiling_chunks() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("abc"), 1);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
    }

    #[test]
    fn status_detail_strips_real_and_escaped_terminal_sequences() {
        let escaped =
            sanitize_status_detail(r"\u001b[26;107Hstatus \u001b[38;2;246;196;83mready\u001b[0m");
        let actual = sanitize_status_detail("\x1b[31mfailed\x1b[0m after check");

        assert_eq!(escaped, "status ready");
        assert_eq!(actual, "failed after check");
        assert!(!escaped.contains(r"\u001b"));
        assert!(!actual.contains('\x1b'));
    }

    #[test]
    fn extracts_social_login_urls() {
        assert_eq!(
            extract_auth_urls("Open https://example.com/callback?code=123, then continue"),
            vec!["https://example.com/callback?code=123"]
        );
        assert_eq!(
            extract_auth_urls("deeplink: claude://login/complete."),
            vec!["claude://login/complete"]
        );
    }

    #[test]
    fn parses_provider_token_usage() {
        let stdout = r#"{"usage":{"input_tokens":12,"output_tokens":8,"total_tokens":20}}"#;
        let usage = extract_token_usage(stdout, "hello", "world").unwrap();
        assert_eq!(usage.input, 12);
        assert_eq!(usage.output, 8);
        assert_eq!(usage.total, 20);
        assert!(!usage.estimated);
    }

    #[test]
    fn extracts_tool_usage_from_provider_streams() {
        let stdout = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"git status"}}]}}"#;
        let tools = extract_tool_usage("claude", stdout, "");
        assert!(tools.iter().any(|tool| tool.name == "Bash"));
    }

    #[test]
    fn provider_stream_json_renders_readable_messages() {
        let claude = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Inspecting the truth predicates now."},{"type":"tool_use","name":"Bash","input":{"command":"git status -sb"}}]}}"#;
        let codex = r#"{"type":"item.completed","item":{"id":"item_13","type":"command_execution","command":"/bin/bash -lc 'sed -n 1,40p execution/live_router.py'","aggregated_output":"def route_order():\n    pass\n"}}"#;
        let gemini = r#"{"candidates":[{"content":{"parts":[{"text":"Gate waterfall evidence is still thin."}]}}]}"#;

        assert_eq!(
            provider_visible_stream(Some("claude"), claude, &[]),
            StreamDisplay::Visible(
                "assistant: Inspecting the truth predicates now. | tool: Bash {\"command\":\"git status -sb\"}"
                    .to_string()
            )
        );
        let codex_visible = match provider_visible_stream(Some("codex"), codex, &[]) {
            StreamDisplay::Visible(text) => text,
            StreamDisplay::Suppress => panic!("codex command event should be visible"),
        };
        assert!(codex_visible.contains("tool: shell"));
        assert!(codex_visible.contains("execution/live_router.py"));
        assert!(!codex_visible.contains("\"type\":\"item.completed\""));
        assert_eq!(
            provider_visible_stream(Some("gemini"), gemini, &[]),
            StreamDisplay::Visible("assistant: Gate waterfall evidence is still thin.".to_string())
        );
    }

    #[test]
    fn provider_stream_strips_escaped_terminal_sequences() {
        let raw = r"\u001b[26;107Hstatus \u001b[38;2;246;196;83mready\u001b[0m";
        assert_eq!(
            provider_visible_stream(Some("codex"), raw, &[]),
            StreamDisplay::Visible("status ready".to_string())
        );

        let codex = r#"{"type":"item.completed","item":{"id":"item_8","type":"command_execution","command":"/bin/bash -lc 'free -h'","aggregated_output":"\\u001b[26;107Htotal used free\\u001b[0m"}}"#;
        let visible = match provider_visible_stream(Some("codex"), codex, &[]) {
            StreamDisplay::Visible(text) => text,
            StreamDisplay::Suppress => panic!("codex command output should be visible"),
        };

        assert!(visible.contains("tool: shell"));
        assert!(visible.contains("total used free"));
        assert!(!visible.contains(r"\u001b"));
        assert!(!visible.contains("[26;107H"));
    }

    #[test]
    fn claude_stream_event_json_suppresses_input_shards() {
        let input_delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":6,"delta":{"type":"input_json_delta","partial_json":"\"fallback_trigger\""},"session_id":"abc"}}"#;
        let tool_start = r#"{"type":"stream_event","event":{"type":"content_block_start","index":6,"content_block":{"type":"tool_use","id":"toolu_123","name":"Agent","input":{"description":"Agent E - CLOB/WS Timing","prompt":"Inspect the live VPS timing path"}},"session_id":"abc"}}"#;
        let text_delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"I found the first timing issue."},"session_id":"abc"}}"#;

        assert_eq!(
            provider_visible_stream(Some("claude"), input_delta, &[]),
            StreamDisplay::Suppress
        );
        assert!(extract_tool_usage("claude", input_delta, "").is_empty());

        let visible_tool = match provider_visible_stream(Some("claude"), tool_start, &[]) {
            StreamDisplay::Visible(text) => text,
            StreamDisplay::Suppress => panic!("tool start should be visible"),
        };
        assert!(visible_tool.contains("tool: Agent"));
        assert!(visible_tool.contains("CLOB/WS Timing"));
        assert!(!visible_tool.contains("stream_event"));
        assert!(!visible_tool.contains("partial_json"));

        assert_eq!(
            provider_visible_stream(Some("claude"), text_delta, &[]),
            StreamDisplay::Visible("assistant: I found the first timing issue.".to_string())
        );
        assert_eq!(
            parse_claude_output(&format!("{text_delta}\n{text_delta}\n")),
            "I found the first timing issue.I found the first timing issue."
        );
    }

    #[test]
    fn provider_stream_json_suppresses_session_plumbing() {
        let claude_hook = r#"{"type":"system","subtype":"hook_response","hook_name":"SessionStart:startup","output":"noise"}"#;
        let codex_session = r#"{"type":"session.started","session_id":"abc"}"#;

        assert_eq!(
            provider_visible_stream(Some("claude"), claude_hook, &[]),
            StreamDisplay::Suppress
        );
        assert_eq!(
            provider_visible_stream(Some("codex"), codex_session, &[]),
            StreamDisplay::Suppress
        );
    }

    #[test]
    fn builds_real_sub_agent_handoff_prompt() {
        let agent = EngineResult {
            name: "codex".to_string(),
            bin: Some("codex".to_string()),
            status: "ok".to_string(),
            duration_ms: 1,
            detail: String::new(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            output: "inspect parser".to_string(),
            command: "codex exec".to_string(),
            token_usage: token_usage("a", "b"),
            tool_calls: vec![],
            sub_agents: vec![],
            role: "executor:sub-agent-1".to_string(),
            iteration: 1,
            total_iterations: 1,
            team_size: 0,
        };
        let prompt = build_team_lead_prompt("ship it", &[agent]);
        assert!(prompt.contains("Sub-agent handoffs"));
        assert!(prompt.contains("inspect parser"));
        assert!(prompt.contains("Original provider prompt"));
    }

    #[test]
    fn json_stream_events_emit_progress_before_final_result() {
        let bus = RuntimeEventBus::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        bus.add_sink(Arc::new(move |event| {
            captured.lock().unwrap().push(event);
        }));
        let progress = bus.progress_sink();

        progress(progress_event_with_context(
            None,
            None,
            ProgressStage::Start,
            None,
            Some(1),
            Some(1),
            false,
            None,
            None,
            vec![],
            "[amon-hen] iteration 1/1 started",
        ));
        progress(progress_event_with_context(
            Some("codex"),
            Some("planner"),
            ProgressStage::Done,
            Some("ok"),
            Some(1),
            Some(1),
            false,
            Some(25),
            Some(token_usage("prompt", "answer")),
            vec![],
            "[amon-hen] done codex role=planner status=ok",
        ));
        let result = test_amon_hen_result(vec![test_engine_result("codex", "planner", 1)], 1);
        bus.emit_result(&result);

        let events = events.lock().unwrap();
        assert!(events.len() >= 3);
        assert!(events[..events.len() - 1]
            .iter()
            .any(|event| event.progress.stage != ProgressStage::Done));
        assert!(events
            .iter()
            .any(|event| event.progress.kind == RuntimeEventKind::TokenUsage));
        assert_eq!(
            events.last().unwrap().progress.kind,
            RuntimeEventKind::Result
        );
        assert!(events.last().unwrap().result.is_some());
        for event in events.iter() {
            serde_json::to_string(event).expect("json-stream event should serialize");
        }
    }

    #[test]
    fn runtime_event_bus_preserves_stream_sequence_order_under_fanout() {
        let bus = RuntimeEventBus::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        bus.add_sink(Arc::new(move |event| {
            captured.lock().unwrap().push(event.sequence);
        }));
        let barrier = Arc::new(std::sync::Barrier::new(16));
        let mut handles = Vec::new();

        for index in 0..16 {
            let progress = bus.progress_sink();
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                progress(progress_event(
                    Some("codex"),
                    Some("planner"),
                    ProgressStage::Heartbeat,
                    Some("running"),
                    format!("[amon-hen] fanout event {index}"),
                ));
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }

        let sequences = events.lock().unwrap().clone();
        assert_eq!(sequences, (1..=16).collect::<Vec<_>>());
    }

    #[test]
    fn multiple_iterations_preserve_timeline_entries() {
        let first = vec![test_engine_result("codex", "planner", 1)];
        let second = vec![test_engine_result("claude", "lead", 2)];
        let timeline = vec![
            iteration_record(1, 2, first.clone(), 10, None, None),
            iteration_record(
                2,
                2,
                second.clone(),
                15,
                iteration_handoff_context(&first, DEFAULT_MAX_MEMBER_CHARS),
                Some("summary context".to_string()),
            ),
        ];
        let result = test_amon_hen_result_with_iterations(second, timeline);

        assert_eq!(result.iterations.len(), 2);
        assert_eq!(result.iterations[0].iteration, 1);
        assert_eq!(result.iterations[1].iteration, 2);
        assert!(result.iterations[1].handoff_context.is_some());
        assert_eq!(result.members[0].name, "claude");
    }

    #[test]
    fn sub_agent_roles_remain_correct_in_timeline() {
        let mut member = test_engine_result("gemini", "lead", 1);
        member.sub_agents = vec![
            test_engine_result("gemini", "lead:sub-agent-1", 1),
            test_engine_result("gemini", "lead:sub-agent-2", 1),
        ];
        let timeline = vec![iteration_record(1, 1, vec![member], 20, None, None)];

        assert_eq!(
            timeline[0]
                .sub_agents
                .iter()
                .map(|agent| agent.role.as_str())
                .collect::<Vec<_>>(),
            vec!["lead:sub-agent-1", "lead:sub-agent-2"]
        );
        assert_eq!(timeline[0].members[0].role, "lead");
    }

    #[test]
    fn runtime_role_matrix_reaches_json_result_and_stream_events() {
        let _lock = ENV_LOCK.lock().unwrap();
        let Some(_env) = ProviderEnvGuard::install_echo_bins() else {
            return;
        };
        let args = CliArgs::try_parse_from([
            "amon-hen",
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
            "--codex-sub-agents",
            "2",
            "--claude-sub-agents",
            "1",
            "--gemini-sub-agents",
            "0",
            "--timeout",
            "5",
            "role matrix smoke",
        ])
        .unwrap();
        let resolved = resolve_args(args).unwrap();
        let bus = RuntimeEventBus::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        bus.add_sink(Arc::new(move |event| {
            captured.lock().unwrap().push(event);
        }));
        let prompt_context =
            build_prompt_context_with_progress(&resolved, Some(bus.progress_sink())).unwrap();
        let result = run_amon_hen_with_progress_and_cancel(
            &resolved,
            prompt_context.prompt,
            prompt_context.commands,
            Some(bus.progress_sink()),
            None,
        );
        bus.emit_result(&result);

        assert_eq!(result.workflow.iterations, 2);
        assert_eq!(result.workflow.teams.get("codex"), Some(&2));
        assert_eq!(result.workflow.teams.get("claude"), Some(&1));
        assert_eq!(result.workflow.teams.get("gemini"), Some(&0));
        assert_eq!(result.iterations.len(), 2);
        for iteration in &result.iterations {
            assert_eq!(
                iteration
                    .members
                    .iter()
                    .map(|member| (member.name.as_str(), member.role.as_str()))
                    .collect::<Vec<_>>(),
                vec![
                    ("codex", "planner"),
                    ("gemini", "executor"),
                    ("claude", "lead")
                ]
            );
            assert_eq!(
                iteration
                    .sub_agents
                    .iter()
                    .map(|agent| (agent.name.as_str(), agent.role.as_str()))
                    .collect::<Vec<_>>(),
                vec![
                    ("codex", "planner:sub-agent-1"),
                    ("codex", "planner:sub-agent-2"),
                    ("claude", "lead:sub-agent-1")
                ]
            );
        }

        let events = events.lock().unwrap();
        assert!(events.iter().any(|event| {
            event.progress.provider.as_deref() == Some("codex")
                && event.progress.role.as_deref() == Some("planner")
                && !event.progress.is_sub_agent
        }));
        assert!(events.iter().any(|event| {
            event.progress.provider.as_deref() == Some("claude")
                && event.progress.role.as_deref() == Some("lead:sub-agent-1")
                && event.progress.is_sub_agent
        }));
        let final_event = events.last().expect("final result event should exist");
        assert_eq!(final_event.progress.kind, RuntimeEventKind::Result);
        assert!(final_event.result.is_some());
    }

    #[test]
    fn per_provider_sub_agent_overrides_are_reflected_in_prompts() {
        let args = CliArgs::try_parse_from([
            "amon-hen",
            "--members",
            "codex,gemini",
            "--team-work",
            "2",
            "--codex-sub-agents",
            "0",
            "--gemini-sub-agents",
            "3",
            "ship",
        ])
        .unwrap();
        let resolved = resolve_args(args).unwrap();
        let workflow = build_workflow(&resolved);
        let codex_prompt = build_member_prompt(MemberPromptInput {
            query: "ship",
            role: "executor",
            workflow: &workflow,
            iteration: 1,
            team_size: effective_team_size(&workflow, "codex"),
            previous_iteration: &[],
            handoff_results: &[],
            plan_output: "",
        });
        let gemini_prompt = build_member_prompt(MemberPromptInput {
            query: "ship",
            role: "executor",
            workflow: &workflow,
            iteration: 1,
            team_size: effective_team_size(&workflow, "gemini"),
            previous_iteration: &[],
            handoff_results: &[],
            plan_output: "",
        });

        assert!(!codex_prompt.contains("Team work:"));
        assert!(gemini_prompt.contains("up to 3 internal sub-agents"));
    }

    #[test]
    fn duplicate_members_are_deduped_in_input_order() {
        let args = CliArgs::try_parse_from([
            "amon-hen",
            "--members",
            "codex,claude,codex,gemini,claude",
            "hello",
        ])
        .unwrap();
        let resolved = resolve_args(args).unwrap();
        assert_eq!(resolved.members, vec!["codex", "claude", "gemini"]);
    }

    #[test]
    fn command_cancel_stops_child_process_promptly() {
        if !command_available("sh") {
            return;
        }
        let cwd = std::env::current_dir().unwrap();
        let cancel = Arc::new(AtomicBool::new(true));
        let started = Instant::now();
        let args = ["-c".to_string(), "sleep 30".to_string()];
        let result =
            run_command(CommandRequest::new("sh", &args, &cwd, 30_000).cancel(Some(cancel)));

        assert!(result.cancelled);
        assert_eq!(result.error.as_deref(), Some("cancelled"));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "cancelled command took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn command_streams_visible_output_and_live_token_usage() {
        if !command_available("sh") {
            return;
        }
        let cwd = std::env::current_dir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let progress: ProgressSink = Arc::new(move |event| {
            captured.lock().unwrap().push(event);
        });
        let args = [
            "-c".to_string(),
            "printf 'visible-work\\n'; printf '{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Bash\"}]}}\\n'"
                .to_string(),
        ];

        let result = run_command(CommandRequest::new("sh", &args, &cwd, 5_000).progress(Some(
            CommandProgress {
                label: "codex planner:sub-agent-1 iteration 1/1".to_string(),
                sink: Some(progress),
                input_tokens: 10,
            },
        )));

        assert_eq!(result.code, Some(0));
        let events = events.lock().unwrap();
        assert!(events.iter().any(|event| {
            event.message.contains("visible-work")
                && event
                    .token_usage
                    .as_ref()
                    .is_some_and(|usage| usage.total > 10)
        }));
        assert!(events.iter().any(|event| {
            event.kind == RuntimeEventKind::ToolUsage
                && event.tool_calls.iter().any(|tool| tool.name == "Bash")
        }));
    }

    #[test]
    fn command_stream_sampling_preserves_important_tool_events() {
        if !command_available("sh") {
            return;
        }
        let cwd = std::env::current_dir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let progress: ProgressSink = Arc::new(move |event| {
            captured.lock().unwrap().push(event);
        });
        let args = [
            "-c".to_string(),
            "i=0; while [ $i -lt 120 ]; do echo stream-line-$i; i=$((i+1)); done; printf '{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Bash\"}]}}\\n'"
                .to_string(),
        ];

        let result = run_command(CommandRequest::new("sh", &args, &cwd, 5_000).progress(Some(
            CommandProgress {
                label: "codex planner iteration 1/1".to_string(),
                sink: Some(progress),
                input_tokens: 10,
            },
        )));

        assert_eq!(result.code, Some(0));
        let events = events.lock().unwrap();
        let streaming_events = events
            .iter()
            .filter(|event| event.status.as_deref() == Some("streaming"))
            .count();
        assert!(
            streaming_events < 20,
            "expected sampled streaming events, got {streaming_events}"
        );
        assert!(events.iter().any(|event| {
            event.kind == RuntimeEventKind::ToolUsage
                && event.tool_calls.iter().any(|tool| tool.name == "Bash")
        }));
    }

    #[test]
    fn command_stream_decodes_provider_json_before_dashboard_events() {
        if !command_available("sh") {
            return;
        }
        let cwd = std::env::current_dir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let progress: ProgressSink = Arc::new(move |event| {
            captured.lock().unwrap().push(event);
        });
        let args = [
            "-c".to_string(),
            "printf '%s\\n' '{\"type\":\"session.started\",\"session_id\":\"abc\"}'; printf '%s\\n' '{\"type\":\"item.completed\",\"item\":{\"id\":\"item_13\",\"type\":\"command_execution\",\"command\":\"/bin/bash -lc sed\",\"aggregated_output\":\"line one\"}}'"
                .to_string(),
        ];

        let result = run_command(CommandRequest::new("sh", &args, &cwd, 5_000).progress(Some(
            CommandProgress {
                label: "codex executor iteration 1/1".to_string(),
                sink: Some(progress),
                input_tokens: 10,
            },
        )));

        assert_eq!(result.code, Some(0));
        let events = events.lock().unwrap();
        assert!(!events
            .iter()
            .any(|event| event.message.contains("session.started")));
        assert!(events.iter().any(|event| {
            event.message.contains("tool: shell")
                && event.message.contains("line one")
                && !event.message.contains("\"type\":\"item.completed\"")
        }));
    }

    #[test]
    fn claude_text_deltas_stream_as_coalesced_live_answer() {
        if !command_available("sh") {
            return;
        }
        let cwd = std::env::current_dir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let progress: ProgressSink = Arc::new(move |event| {
            captured.lock().unwrap().push(event);
        });
        let script = concat!(
            "printf '%s\\n' '{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Reviewing \"}}}';",
            "printf '%s\\n' '{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"gates now.\"}}}'"
        );
        let args = ["-c".to_string(), script.to_string()];

        let result = run_command(CommandRequest::new("sh", &args, &cwd, 5_000).progress(Some(
            CommandProgress {
                label: "claude lead+planner iteration 1/1".to_string(),
                sink: Some(progress),
                input_tokens: 10,
            },
        )));

        assert_eq!(result.code, Some(0));
        let events = events.lock().unwrap();
        assert!(events.iter().any(|event| {
            event
                .message
                .contains("assistant live: Reviewing gates now.")
                && event.provider.as_deref() == Some("claude")
        }));
        assert!(!events.iter().any(|event| {
            event.message.contains("stdout: assistant: Reviewing ")
                || event.message.contains("stdout: assistant: gates now.")
        }));
    }

    #[test]
    fn missing_provider_preflight_avoids_team_fanout() {
        let args = CliArgs::try_parse_from([
            "amon-hen",
            "--members",
            "codex",
            "--team-work",
            "5",
            "hello",
        ])
        .unwrap();
        let resolved = resolve_args(args).unwrap();
        let options = EngineRunOptions {
            prompt: "hello".to_string(),
            cwd: resolved.cwd.clone(),
            timeout_ms: 1_000,
            effort: None,
            model: None,
            permission: None,
            auth: DEFAULT_AUTH_MODE.to_string(),
            capability: provider_capability(&resolved, "codex"),
            role: "planner".to_string(),
            iteration: 1,
            total_iterations: 1,
            team_size: 5,
            is_sub_agent: false,
            live: false,
            progress: None,
            cancel: None,
        };

        let result = run_engine("definitely-missing-amon-hen-test-bin", options);

        assert_eq!(result.status, "missing");
        assert!(result.sub_agents.is_empty());
        assert!(result.detail.contains("not found in PATH"));
    }

    #[test]
    fn system_time_is_available_for_test_environment() {
        assert!(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .is_ok());
    }

    #[test]
    fn cli_hygiene_has_no_legacy_npm_cli_or_council_binary_references() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo_root = crate_dir
            .parent()
            .and_then(Path::parent)
            .expect("crate lives under crates/amon-hen");
        assert!(!repo_root.join("package.json").exists());
        assert!(!repo_root.join("package-lock.json").exists());
        assert!(!repo_root.join("npm").exists());
        assert!(!repo_root.join("bin").join("council").exists());

        let docs = [
            repo_root.join("README.md"),
            crate_dir.join("README.md"),
            crate_dir.join("CHANGELOG.md"),
        ];
        for path in docs {
            let text = fs::read_to_string(&path).unwrap();
            assert!(
                !text.contains("`council`") && !text.contains(" council "),
                "legacy binary reference remains in {}",
                path.display()
            );
        }

        let help = render_cli_help();
        assert!(!help.contains("council"));
        assert!(help.contains("amon-hen"));
    }

    fn test_amon_hen_result(members: Vec<EngineResult>, total_iterations: usize) -> AmonHenResult {
        let timeline = vec![iteration_record(
            total_iterations,
            total_iterations,
            members.clone(),
            25,
            None,
            Some("summary context".to_string()),
        )];
        test_amon_hen_result_with_iterations(members, timeline)
    }

    fn test_amon_hen_result_with_iterations(
        members: Vec<EngineResult>,
        iterations: Vec<IterationRecord>,
    ) -> AmonHenResult {
        let mut teams = HashMap::new();
        teams.insert("codex".to_string(), 0);
        teams.insert("claude".to_string(), 0);
        teams.insert("gemini".to_string(), 0);
        let workflow = Workflow {
            handoff: true,
            lead: Some("claude".to_string()),
            planner: Some("codex".to_string()),
            planner_mode: PLANNER_MODE_BLOCKING.to_string(),
            iterations: iterations.len().max(1),
            team_work: 0,
            teams,
        };
        AmonHenResult {
            query: "ship it".to_string(),
            cwd: ".".to_string(),
            members_requested: members.iter().map(|member| member.name.clone()).collect(),
            summarizer_requested: "auto".to_string(),
            workflow,
            prompt_commands: vec![],
            iterations,
            members,
            summary: test_engine_result("codex", "summary", 1),
        }
    }

    fn test_engine_result(name: &str, role: &str, iteration: usize) -> EngineResult {
        EngineResult {
            name: name.to_string(),
            bin: Some(name.to_string()),
            status: "ok".to_string(),
            duration_ms: 10,
            detail: String::new(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            output: format!("{name} output"),
            command: format!("{name} exec"),
            token_usage: token_usage("prompt", "output"),
            tool_calls: vec![ToolUsage {
                name: "tool".to_string(),
                kind: name.to_string(),
                status: "observed".to_string(),
                detail: "detail".to_string(),
            }],
            sub_agents: vec![],
            role: role.to_string(),
            iteration,
            total_iterations: iteration.max(1),
            team_size: 0,
        }
    }
}

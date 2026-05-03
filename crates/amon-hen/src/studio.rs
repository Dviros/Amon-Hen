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

const MENU: [&str; 15] = [
    "Run / re-run",
    "Edit prompt",
    "Social login",
    "Auth status",
    "Linear status",
    "Deliver Linear",
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
}

#[derive(Debug, Clone)]
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
    status: String,
    input_mode: Option<InputMode>,
    input_buffer: String,
    show_help: bool,
    exit_armed_until: Option<Instant>,
}

enum StudioAction {
    None,
    RunAmonHen,
    SocialLogin,
    AuthStatus,
    CapabilitiesStatus,
    LinearStatus,
    LinearDeliver,
    Quit,
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
        status: "Ready".to_string(),
        input_mode: None,
        input_buffer: String::new(),
        show_help: false,
        exit_armed_until: None,
    };

    configure_studio_color(&state.resolved.raw);

    let mut guard = match TerminalGuard::enter() {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };

    loop {
        if let Err(error) = draw(&state) {
            drop(guard);
            eprintln!("{error}");
            return 1;
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
                run_external_action(&mut guard, || run_amon_hen_from_studio(&mut state));
            }
            StudioAction::SocialLogin => {
                run_external_action(&mut guard, || match run_social_login(&state.resolved) {
                    Ok(()) => state.status = "Social login completed".to_string(),
                    Err(error) => state.status = format!("Social login failed: {error}"),
                });
            }
            StudioAction::AuthStatus => {
                run_external_action(&mut guard, || {
                    state.last_auth_result = Some(render_auth_statuses(&collect_auth_statuses(
                        &state.resolved,
                    )));
                    state.status = "Auth status refreshed".to_string();
                    state.focus = Pane::Agents;
                });
            }
            StudioAction::CapabilitiesStatus => {
                run_external_action(&mut guard, || {
                    state.last_capability_result = Some(render_provider_capability_statuses(
                        &collect_provider_capability_statuses(&state.resolved),
                    ));
                    state.status = "Provider capabilities refreshed".to_string();
                    state.focus = Pane::Capabilities;
                });
            }
            StudioAction::LinearStatus => {
                run_external_action(&mut guard, || {
                    match linear_delivery::get_linear_status(&state.resolved) {
                        Ok(status) => {
                            state.last_linear_result =
                                Some(linear_delivery::render_linear_status(&status));
                            state.status = "Linear status refreshed".to_string();
                            state.focus = Pane::Linear;
                        }
                        Err(error) => state.status = format!("Linear status failed: {error}"),
                    }
                });
            }
            StudioAction::LinearDeliver => {
                run_external_action(&mut guard, || {
                    state.resolved.raw.deliver_linear = true;
                    match linear_delivery::run_linear_delivery(&state.resolved) {
                        Ok(result) => {
                            state.last_linear_result =
                                Some(linear_delivery::render_linear_delivery_result(&result));
                            state.status = if result.success {
                                "Linear delivery completed".to_string()
                            } else {
                                "Linear delivery needs attention".to_string()
                            };
                            state.focus = Pane::Linear;
                        }
                        Err(error) => state.status = format!("Linear delivery failed: {error}"),
                    }
                });
            }
        }
    }
}

fn run_external_action(guard: &mut TerminalGuard, action: impl FnOnce()) {
    let _ = execute!(io::stderr(), Show, LeaveAlternateScreen);
    let _ = disable_raw_mode();
    action();
    let _ = enable_raw_mode();
    let _ = execute!(
        io::stderr(),
        EnterAlternateScreen,
        Hide,
        Clear(ClearType::All)
    );
    let _ = guard;
}

fn run_amon_hen_from_studio(state: &mut StudioState) {
    state.status = "Running Amon Hen...".to_string();
    let mut resolved = state.resolved.clone();
    resolved.prompt = state.prompt.clone();
    let prompt_context = match build_prompt_context(&resolved) {
        Ok(context) => context,
        Err(error) => {
            state.status = format!("Prompt context failed: {error}");
            return;
        }
    };
    let result = run_amon_hen(&resolved, prompt_context.prompt, prompt_context.commands);
    state.status = if is_success(&result) {
        "Amon Hen run completed".to_string()
    } else {
        "Amon Hen run needs attention".to_string()
    };
    state.last_result = Some(result);
    state.focus = Pane::Results;
}

fn handle_event(state: &mut StudioState, event: Event) -> Result<StudioAction, String> {
    let Event::Key(key) = event else {
        return Ok(StudioAction::None);
    };
    if let Some(mode) = state.input_mode.clone() {
        return handle_input_event(state, key, mode);
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
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
        "Edit prompt" => start_input(state, InputMode::Prompt, state.prompt.clone()),
        "Social login" => Ok(StudioAction::SocialLogin),
        "Auth status" => Ok(StudioAction::AuthStatus),
        "Linear status" => Ok(StudioAction::LinearStatus),
        "Deliver Linear" => Ok(StudioAction::LinearDeliver),
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
        Line::from(format!("Timeout: {}s", state.resolved.raw.timeout)),
        Line::from(format!("Repo: {}", display_cwd(&state.resolved.cwd))),
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
        .map(|member| provider_result(state, member).map_or(0, |result| result.token_usage.total))
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
    let status = result.map_or("ready", |result| result.status.as_str());
    let token_usage = result.map(|result| &result.token_usage);
    let total_tokens = token_usage.map_or(0, |usage| usage.total);
    let percent = ((total_tokens.saturating_mul(100)) / max_tokens).min(100) as u16;
    let tools = result.map_or(0, |result| result.tool_calls.len());
    let sub_agents = result.map_or(0, |result| result.sub_agents.len());
    let command = result
        .map(|result| studio_clip(&result.command, 64))
        .unwrap_or_else(|| "not run yet".to_string());
    let block = panel_block(member.to_ascii_uppercase(), state.focus == Pane::Agents)
        .border_style(Style::default().fg(color))
        .title_style(strong(color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
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
            Span::raw(provider_auth(&state.resolved, member)),
            Span::raw("  "),
            Span::styled("effort ", muted()),
            Span::raw(provider_effort(state, member)),
        ]),
        Line::from(vec![
            Span::styled("model ", muted()),
            Span::raw(studio_clip(&provider_model(state, member), 28)),
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
            Row::new(vec![
                member.to_string(),
                result.map_or("ready".to_string(), |result| result.status.clone()),
                result.map_or("0".to_string(), |result| {
                    compact_count(result.token_usage.input)
                }),
                result.map_or("0".to_string(), |result| {
                    compact_count(result.token_usage.output)
                }),
                result.map_or("0".to_string(), |result| {
                    compact_count(result.token_usage.total)
                }),
                result.map_or("0".to_string(), |result| {
                    result.tool_calls.len().to_string()
                }),
                result.map_or("0".to_string(), |result| {
                    result.sub_agents.len().to_string()
                }),
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
            Line::from("r runs, e edits prompt, ? toggles help."),
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
        "err" | "timeout" | "missing" => STUDIO_RED,
        "ready" => STUDIO_MUTED,
        _ => STUDIO_GOLD,
    }
}

fn provider_result<'a>(state: &'a StudioState, member: &str) -> Option<&'a EngineResult> {
    state
        .last_result
        .as_ref()?
        .members
        .iter()
        .find(|result| result.name == member)
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
    let Some(result) = &state.last_result else {
        return vec!["No run yet".to_string()];
    };
    let mut lines = result
        .members
        .iter()
        .map(|member| {
            format!(
                "{} [{}] role:{} tokens:{}",
                member.name, member.status, member.role, member.token_usage.total
            )
        })
        .collect::<Vec<_>>();
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
    state
        .last_result
        .as_ref()
        .map(|result| result.members.len() + 2)
        .unwrap_or(1)
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
        AmonHenResult {
            query: state.prompt.clone(),
            cwd: state.resolved.cwd.display().to_string(),
            members_requested: state.resolved.members.clone(),
            summarizer_requested: state.resolved.raw.summarizer.clone(),
            workflow: build_workflow(&state.resolved),
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
            members: vec![codex, claude, gemini],
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
            status: "Ready".to_string(),
            input_mode: None,
            input_buffer: String::new(),
            show_help: false,
            exit_armed_until: None,
        }
    }
}

use std::io::stdout;
use std::sync::Arc;

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::Terminal;

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, List, ListItem, ListState, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Wrap,
};
use ratatui::Frame;
use tokio::sync::{mpsc, Mutex};

use crate::config::Config;
use crate::history::Role;
use crate::llm::{LlmClient, StreamEvent};
use crate::tools::ToolCall;
use crate::Agent;
use crate::{CommandAction, CommandContext, ViewBlock, ViewSpec};

// events from the background streaming task to the UI loop
enum UiEvent {
    Delta(String),
    Reasoning(String),
    RoundDone,
    ToolResult { name: String, command: Option<String>, preview: String },
    Context,
    Status(String),
    Error(String),
    TurnDone,
}

#[derive(Clone)]
struct TuiMessage {
    role: Role,
    content: String,
    /// executed command for tool cards (bash etc).
    command: Option<String>,
    /// originating tool name for tool cards.
    tool: Option<String>,
}

struct ContextViewerState {
    view_name: String,
    spec: ViewSpec,
    selected: usize,
    detail_scroll: u16,
    detail_focus: bool,
    confirm_drop: bool,
    confirm_clear: bool,
}

// interactive model-selection overlay state.
struct ModelPickerState {
    models: Vec<String>,
    selected: usize,
    current: String,
}

pub struct TuiAgent {
    agent: Arc<Mutex<Agent>>,
    /// shared llm client so the turn task can stream without locking the agent.
    llm: Arc<dyn LlmClient>,
    messages: Vec<TuiMessage>,
    streaming: String,
    reasoning: String,
    input: String,
    status: String,
    busy: bool,
    chat_scroll: u16,
    follow_bottom: bool,
    last_max_scroll: usize,
    ui_rx: Option<mpsc::Receiver<UiEvent>>,
    model_name: String,
    context_viewer: Option<ContextViewerState>,
    model_picker: Option<ModelPickerState>,
    reasoning_visible: bool,
    slash_selection: usize,
    // cached values updated from async context — draw reads these instead of locking
    cached_cwd: String,
    cached_tool_count: usize,
    cached_command_names: Vec<String>,
    cached_total_tokens: usize,
    cached_limit_tokens: usize,
    cached_context_pct: usize,
}

impl TuiAgent {
    pub fn new(agent: Agent) -> Self {
        let llm = agent.llm.clone();
        let model_name = agent.config.model.clone();
        let cwd = readable_path(&agent.config.repo_root);
        Self {
            agent: Arc::new(Mutex::new(agent)),
            llm,
            messages: Vec::new(),
            streaming: String::new(),
            reasoning: String::new(),
            input: String::new(),
            status: "ready".to_string(),
            busy: false,
            chat_scroll: 0,
            follow_bottom: true,
            last_max_scroll: 0,
            ui_rx: None,
            model_name,
            context_viewer: None,
            model_picker: None,
            reasoning_visible: true,
            slash_selection: 0,
            cached_cwd: cwd,
            cached_tool_count: 0,
            cached_command_names: Vec::new(),
            cached_total_tokens: 0,
            cached_limit_tokens: crate::config::CONTEXT_LIMIT_TOKENS,
            cached_context_pct: 0,
        }
    }

    /// refresh cached values from the agent. must be called from an async
    /// context (uses `.lock().await`). draw methods read only cached fields.
    async fn refresh_cache(&mut self) {
        let agent = self.agent.lock().await;
        self.cached_cwd = readable_path(&agent.config.repo_root);
        self.cached_tool_count = agent.tools.schemas().len();
        self.cached_command_names = agent.commands.names();
        self.model_name = agent.config.model.clone();

        // token estimate from the full request the agent would send.
        let schemas = agent.tools.schemas();
        self.cached_total_tokens = agent.history.estimated_request_tokens(&schemas);
        self.cached_limit_tokens = crate::config::CONTEXT_LIMIT_TOKENS;
        let pct = if self.cached_limit_tokens > 0 {
            ((self.cached_total_tokens as f64 / self.cached_limit_tokens as f64) * 100.0) as usize
        } else {
            0
        };
        self.cached_context_pct = pct;
    }

    async fn open_view(&mut self, name: &str) {
        let spec = {
            let agent = self.agent.lock().await;
            agent.build_view(name)
        };
        match spec {
            Some(spec) => {
                let selected = spec.turns.len().saturating_sub(1);
                self.context_viewer = Some(ContextViewerState {
                    view_name: name.to_string(),
                    spec,
                    selected,
                    detail_scroll: 0,
                    detail_focus: false,
                    confirm_drop: false,
                    confirm_clear: false,
                });
            }
            None => {
                self.messages.push(TuiMessage {
                    role: Role::Assistant,
                    content: format!("no '{name}' view registered"),
                    command: None,
                    tool: None,
                });
            }
        }
    }

    async fn rebuild_context_viewer(&mut self) {
        let name = self.context_viewer.as_ref().map(|v| v.view_name.clone()).unwrap_or_default();
        let spec = {
            let agent = self.agent.lock().await;
            agent.build_view(&name)
        };
        match spec {
            Some(spec) if !spec.turns.is_empty() => {
                if let Some(viewer) = self.context_viewer.as_mut() {
                    viewer.selected = viewer.selected.min(spec.turns.len().saturating_sub(1));
                    viewer.spec = spec;
                    viewer.detail_scroll = 0;
                    viewer.confirm_drop = false;
                    viewer.confirm_clear = false;
                }
            }
            _ => {
                self.context_viewer = None;
            }
        }
    }

    async fn rebuild_messages_from_history(&mut self) {
        let messages: Vec<TuiMessage> = {
            let agent = self.agent.lock().await;
            let hist = &agent.history;
            hist.messages
                .iter()
                .map(|m| {
                    // tool cards draw their name + command from the original tool
                    // call (matched by id), so they survive transcript rebuilds
                    // instead of degrading to anonymous output.
                    let mut tui = TuiMessage {
                        role: m.role,
                        content: m.content.clone(),
                        command: None,
                        tool: None,
                    };
                    if m.role == Role::Tool {
                        if let Some(tcid) = &m.tool_call_id {
                            let found = hist
                                .messages
                                .iter()
                                .flat_map(|x| x.tool_calls.iter())
                                .find(|tc| &tc.id == tcid);
                            if let Some(tc) = found {
                                tui.tool = Some(tc.name.clone());
                                tui.command = tc
                                    .arguments
                                    .get("command")
                                    .and_then(|c| c.as_str())
                                    .map(String::from);
                            }
                        }
                    }
                    tui
                })
                .collect()
        };
        self.messages = messages;
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        let mut terminal = setup_terminal()?;
        let mut events = EventStream::new();
        self.status = format!(
            "model={} cwd={} — type a message, enter to send, ctrl-c to quit",
            {
                let a = self.agent.lock().await;
                a.config.model.clone()
            },
            {
                let a = self.agent.lock().await;
                a.config.repo_root.display().to_string()
            }
        );
        // register built-ins once, synchronously: everything is compiled in,
        // so there is no hot-reload or background load step.
        {
            let mut a = self.agent.lock().await;
            if !a.initialized {
                a.init_builtins();
            }
        }
        self.refresh_cache().await;

        loop {
            terminal.draw(|f| self.draw(f))?;
            tokio::select! {
                maybe_evt = events.next() => {
                    let Some(Ok(evt)) = maybe_evt else { break; };
                    if !self.handle_event(evt).await? { break; }
                }
                Some(ui_evt) = async {
                    match &mut self.ui_rx {
                        Some(rx) => rx.recv().await,
                        None => None,
                    }
                } => {
                    if !self.handle_ui_event(ui_evt).await? { break; }
                }
            }
        }
        restore_terminal()?;
        Ok(())
    }

    fn draw(&mut self, f: &mut Frame<'_>) {
        if let Some(viewer) = &self.context_viewer {
            self.draw_context_viewer(f, viewer);
            return;
        }
        if let Some(picker) = &self.model_picker {
            self.draw_model_picker(f, picker);
            return;
        }

        let area = f.area();
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(area);

        self.draw_header_bar(f, chunks[0]);
        self.draw_chat_area(f, chunks[1]);
        self.draw_context_meter(f, chunks[2]);
        self.draw_input_area(f, chunks[3]);
        self.draw_footer_bar(f, chunks[4]);
    }

    fn draw_header_bar(&self, f: &mut Frame<'_>, area: Rect) {
        let sep = Span::styled(" │ ", Style::default().fg(Color::DarkGray));
        let status = Line::from(vec![
            Span::styled(
                self.model_name.clone(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            sep.clone(),
            Span::styled(self.cached_cwd.clone(), Style::default().fg(Color::White)),
            sep.clone(),
            Span::styled(
                format!("{} tools", self.cached_tool_count),
                Style::default().fg(Color::White),
            ),
        ]);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Line::from(vec![
                Span::styled("◆ ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    "code-o-matic",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ]));

        let bar = Paragraph::new(status).block(block).alignment(Alignment::Right);
        f.render_widget(bar, area);
    }

    fn draw_chat_area(&mut self, f: &mut Frame<'_>, area: Rect) {
        let mut lines: Vec<Line<'_>> = Vec::new();
        let width = area.width.saturating_sub(2) as usize;

        for m in &self.messages {
            render_message(&mut lines, m, &self.model_name, width);
        }

        // reasoning line above the streaming answer while busy
        if self.busy && !self.reasoning.is_empty() && self.reasoning_visible {
            let spinner = thinking_spinner();
            push_styled_wrapped(
                &mut lines,
                &format!("{spinner} thinking… {}", self.reasoning),
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                width,
            );
            lines.push(Line::from(""));
        }

        // live streaming text
        if self.busy && !self.streaming.is_empty() {
            render_message(
                &mut lines,
                &TuiMessage {
                    role: Role::Assistant,
                    content: self.streaming.clone(),
                    command: None,
                    tool: None,
                },
                &self.model_name,
                width,
            );
        }

        // thinking indicator before any content arrives
        if self.busy && self.streaming.is_empty() && self.reasoning.is_empty() {
            let spinner = thinking_spinner();
            push_styled_wrapped(
                &mut lines,
                &format!("{spinner} thinking…"),
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                width,
            );
            lines.push(Line::from(""));
        }

        let total = lines.len();
        let height = (area.height.saturating_sub(2)) as usize;
        let max_scroll = total.saturating_sub(height).min(u16::MAX as usize) as u16;
        self.last_max_scroll = max_scroll as usize;
        let scroll = if self.follow_bottom { max_scroll } else { self.chat_scroll.min(max_scroll) };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(" conversation ", Style::default().fg(Color::DarkGray)));

        let chat = Paragraph::new(lines).block(block).scroll((scroll, 0));
        f.render_widget(chat, area);

        // vertical scrollbar when content overflows. ratatui's scrollbar only puts
        // the thumb at the bottom when position == content_length - 1, so we normalise
        // the paragraph scroll offset (0..=max_scroll) onto that range.
        if total > height {
            let sb_pos = if max_scroll == 0 {
                0
            } else {
                (scroll as usize).saturating_mul(total.saturating_sub(1)) / (max_scroll as usize)
            };
            let mut state = ScrollbarState::new(total).position(sb_pos);
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None);
            f.render_stateful_widget(
                scrollbar,
                area.inner(Margin { vertical: 1, horizontal: 1 }),
                &mut state,
            );
        }
    }

    fn draw_context_meter(&self, f: &mut Frame<'_>, area: Rect) {
        let (total, limit, pct) = self.context_tokens();
        let ratio = if limit == 0 { 0.0 } else { (total as f64 / limit as f64).min(1.0) };
        let colour = if pct < 50 {
            Color::Green
        } else if pct < 80 {
            Color::Yellow
        } else {
            Color::Red
        };
        let width = area.width.saturating_sub(2).max(1) as usize;
        let fill = ((width as f64) * ratio).round() as usize;
        let label = format!("{total} / {limit} tok ({pct}%)");

        // one-line bar: filled cells carry the colour background, the rest black.
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(width);
        for c in 0..width {
            let bg = if c < fill { colour } else { Color::Black };
            spans.push(Span::styled(" ", Style::default().bg(bg)));
        }
        // overlay the label centred; text flips to black where the fill covers it.
        let pad = width.saturating_sub(label.chars().count()) / 2;
        for (i, ch) in label.chars().enumerate() {
            let c = pad + i;
            if c >= width {
                break;
            }
            let covered = c < fill;
            let fg = if covered { Color::Black } else { Color::White };
            let bg = if covered { colour } else { Color::Black };
            spans[c] = Span::styled(ch.to_string(), Style::default().fg(fg).bg(bg));
        }
        let bar = Paragraph::new(Line::from(spans))
            .block(Block::default().padding(Padding::horizontal(1)));
        f.render_widget(bar, area);
    }

    fn draw_input_area(&self, f: &mut Frame<'_>, area: Rect) {
        let prompt = vec![
            Span::styled("> ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(self.input.clone(), Style::default().fg(Color::White)),
            Span::styled("▏", Style::default().fg(Color::DarkGray)),
        ];
        let mut text_lines = vec![Line::from(prompt)];

        // slash command autocomplete
        if let Some(prefix) = self.slash_prefix() {
            let suggestions = self.slash_suggestions(prefix);
            if !suggestions.is_empty() {
                let visible: Vec<String> = suggestions
                    .into_iter()
                    .enumerate()
                    .take(8)
                    .map(|(i, name)| {
                        if i == self.slash_selection {
                            format!("▸ /{name}  ")
                        } else {
                            format!("  /{name}  ")
                        }
                    })
                    .collect();
                text_lines.push(Line::styled(
                    visible.join("").trim_end().to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(" input ", Style::default().fg(Color::DarkGray)));
        let para = Paragraph::new(Text::from(text_lines)).block(block);
        f.render_widget(para, area);
    }

    fn draw_footer_bar(&self, f: &mut Frame<'_>, area: Rect) {
        let sep = Span::styled(" │ ", Style::default().fg(Color::DarkGray));
        let footer = Line::from(vec![
            Span::styled("enter send", Style::default().fg(Color::White)),
            sep.clone(),
            Span::styled("↑↓ scroll", Style::default().fg(Color::White)),
            sep.clone(),
            Span::styled("/help commands", Style::default().fg(Color::White)),
            sep.clone(),
            Span::styled("/reasoning", Style::default().fg(Color::White)),
        ]);
        let bar = Paragraph::new(Text::from(vec![footer]))
            .style(Style::default().bg(Color::Black))
            .alignment(Alignment::Left);
        f.render_widget(bar, area);
    }

    fn slash_prefix(&self) -> Option<String> {
        if !self.input.starts_with('/') {
            return None;
        }
        let after = &self.input[1..];
        // only show autocomplete when there is no space yet
        if after.contains(' ') {
            return None;
        }
        Some(after.to_string())
    }

    fn slash_suggestions(&self, prefix: String) -> Vec<String> {
        let mut matches: Vec<String> = self
            .cached_command_names
            .iter()
            .filter(|n| n.starts_with(&prefix))
            .take(8)
            .cloned()
            .collect();
        matches.sort();
        matches
    }

    fn context_tokens(&self) -> (usize, usize, usize) {
        (self.cached_total_tokens, self.cached_limit_tokens, self.cached_context_pct)
    }

    fn build_detail_lines(&self, viewer: &ContextViewerState) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let Some(turn) = viewer.spec.turns.get(viewer.selected) else {
            return lines;
        };
        for block in &turn.blocks {
            match block {
                ViewBlock::UserText { text, tokens } => {
                    lines.push(Line::from(format!("user (~{tokens}t)")));
                    lines.push(Line::from(text.clone()));
                }
                ViewBlock::AssistantText { text, tokens } => {
                    lines.push(Line::from(format!("assistant (~{tokens}t)")));
                    lines.push(Line::from(text.clone()));
                }
                ViewBlock::ToolCall { name, input_json, tokens } => {
                    lines.push(Line::from(format!("tool call: {name} (~{tokens}t)")));
                    lines.push(Line::from(input_json.clone()));
                }
                ViewBlock::ToolResult { tool_name, content, tokens } => {
                    lines.push(Line::from(format!("tool result: {tool_name} (~{tokens}t)")));
                    lines.push(Line::from(content.clone()));
                }
            }
            lines.push(Line::from(""));
        }
        lines
    }

    fn draw_context_viewer(&self, f: &mut Frame<'_>, viewer: &ContextViewerState) {
        let area = f.area();
        let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);
        let hchunks = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[0]);

        // left pane: turn list with a status footer inside the border
        let left_block = Block::default().borders(Borders::ALL).title("Turns");
        let left_inner = left_block.inner(hchunks[0]);
        f.render_widget(left_block, hchunks[0]);
        let left_chunks =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(left_inner);

        let items: Vec<ListItem<'_>> = viewer
            .spec
            .turns
            .iter()
            .enumerate()
            .map(|(i, turn)| {
                let base = format!("[turn {}] {} (~{}t)", i, turn.preview, turn.tokens_est);
                let text = if i == viewer.selected { base } else { format!("  {base}") };
                let style = if i == viewer.selected {
                    Style::default().fg(Color::Yellow)
                } else if !turn.in_window {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(Line::from(text)).style(style)
            })
            .collect();
        let list = List::new(items)
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(Color::Yellow));
        let mut list_state = ListState::default();
        list_state.select(Some(viewer.selected));
        f.render_stateful_widget(list, left_chunks[0], &mut list_state);

        let status_text = format!(
            "Total: {}t / {}t ({}%)",
            viewer.spec.total_tokens, viewer.spec.limit_tokens, viewer.spec.context_pct
        );
        let status_para =
            Paragraph::new(status_text).style(Style::default().fg(Color::Yellow).bg(Color::Black));
        f.render_widget(status_para, left_chunks[1]);

        // right pane: detail view of the selected turn
        let right_block = Block::default().borders(Borders::ALL).title("Detail");
        let detail_lines = self.build_detail_lines(viewer);
        let detail_height = hchunks[1].height.saturating_sub(2) as usize;
        let max_scroll =
            detail_lines.len().saturating_sub(detail_height).min(u16::MAX as usize) as u16;
        let scroll = viewer.detail_scroll.min(max_scroll);
        let detail = Paragraph::new(detail_lines)
            .block(right_block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));
        f.render_widget(detail, hchunks[1]);

        // bottom row: confirmation prompt or help
        let bottom_text = if viewer.confirm_drop {
            "Drop this turn? (y/n)".to_string()
        } else if viewer.confirm_clear {
            "Clear all history? (y/n)".to_string()
        } else {
            "↑↓ navigate · Tab focus · d drop · c clear · Esc close".to_string()
        };
        let bottom =
            Paragraph::new(bottom_text).style(Style::default().fg(Color::Yellow).bg(Color::Black));
        f.render_widget(bottom, chunks[1]);
    }

    async fn handle_event(&mut self, evt: Event) -> anyhow::Result<bool> {
        if let Event::Key(k) = evt {
            if k.kind != KeyEventKind::Press {
                return Ok(true);
            }
            // quit shortcuts are always active, even when the context viewer is open
            if k.modifiers.contains(KeyModifiers::CONTROL)
                && (k.code == KeyCode::Char('c') || k.code == KeyCode::Char('d'))
            {
                return Ok(false);
            }
            if self.context_viewer.is_some() {
                return self.handle_context_viewer_key(k).await;
            }
            if self.model_picker.is_some() {
                return self.handle_model_picker_key(k).await;
            }

            // slash autocomplete navigation
            if self.slash_prefix().is_some() {
                match k.code {
                    KeyCode::Tab => {
                        if let Some(prefix) = self.slash_prefix() {
                            let suggestions = self.slash_suggestions(prefix);
                            if !suggestions.is_empty() {
                                let idx = self.slash_selection.min(suggestions.len() - 1);
                                self.input = format!("/{} ", suggestions[idx]);
                                self.slash_selection = 0;
                            }
                        }
                        return Ok(true);
                    }
                    KeyCode::Up => {
                        if self.slash_selection > 0 {
                            self.slash_selection -= 1;
                        }
                        return Ok(true);
                    }
                    KeyCode::Down => {
                        self.slash_selection += 1;
                        return Ok(true);
                    }
                    KeyCode::Esc => {
                        self.input.clear();
                        self.slash_selection = 0;
                        return Ok(true);
                    }
                    _ => {}
                }
            }

            match k.code {
                KeyCode::Enter if !self.busy && !self.input.is_empty() => {
                    let prompt = std::mem::take(&mut self.input);
                    self.slash_selection = 0;
                    if let Some(rest) = prompt.strip_prefix('/') {
                        // slash commands act without a chat bubble; feedback goes
                        // to the status line instead of the conversation.
                        let mut parts = rest.splitn(2, ' ');
                        let cmd_name = parts.next().unwrap_or_default().to_string();
                        let args = parts.next().unwrap_or_default().to_string();
                        let (ctx, commands) = {
                            let agent = self.agent.lock().await;
                            let ctx = CommandContext {
                                model: agent.config.model.clone(),
                                system_prompt: agent
                                    .history
                                    .system_prompt
                                    .clone()
                                    .unwrap_or_default(),
                                args: args.clone(),
                                snapshot: agent.build_conversation_snapshot(),
                                reasoning: self.reasoning_visible,
                                available_models: agent.config.available_models.clone(),
                            };
                            (ctx, agent.commands.clone())
                        };
                        match commands.dispatch(&cmd_name, &ctx) {
                            Err(e) => {
                                self.status = format!("{e}");
                            }
                            Ok(result) => {
                                self.status = result.message;
                                match result.action {
                                    None => {}
                                    Some(CommandAction::OpenView(name)) => {
                                        self.open_view(&name).await;
                                    }
                                    Some(CommandAction::OpenModelPicker) => {
                                        self.open_model_picker().await;
                                    }
                                    Some(CommandAction::ClearHistory) => {
                                        let mut agent = self.agent.lock().await;
                                        agent.history.clear_messages();
                                        drop(agent);
                                        self.messages.clear();
                                        self.refresh_cache().await;
                                    }
                                    Some(CommandAction::UndoLastTurn) => {
                                        let mut agent = self.agent.lock().await;
                                        agent.history.drop_last_turn();
                                        drop(agent);
                                        self.rebuild_messages_from_history().await;
                                        self.refresh_cache().await;
                                    }
                                    Some(CommandAction::SetReasoning(v)) => {
                                        self.reasoning_visible = v;
                                        self.refresh_cache().await;
                                    }
                                    Some(CommandAction::SetModel(m)) => {
                                        let mut agent = self.agent.lock().await;
                                        if let Some(cfg) = Arc::get_mut(&mut agent.config) {
                                            cfg.model = m.clone();
                                            self.model_name = m;
                                        } else {
                                            self.status =
                                                format!("model will change to {m} on restart");
                                        }
                                        drop(agent);
                                        self.refresh_cache().await;
                                    }
                                    Some(CommandAction::Exit) => {
                                        return Ok(false);
                                    }
                                    Some(CommandAction::RunTurn(text)) => {
                                        self.messages.push(TuiMessage {
                                            role: Role::User,
                                            content: text.clone(),
                                            command: None,
                                            tool: None,
                                        });
                                        self.busy = true;
                                        self.streaming.clear();
                                        self.reasoning.clear();
                                        self.follow_bottom = true;
                                        self.status = "thinking".to_string();
                                        self.start_turn(text).await;
                                    }
                                }
                            }
                        }
                    } else {
                        self.messages.push(TuiMessage {
                            role: Role::User,
                            content: prompt.clone(),
                            command: None,
                            tool: None,
                        });
                        self.busy = true;
                        self.streaming.clear();
                        self.reasoning.clear();
                        self.follow_bottom = true;
                        self.status = "thinking".to_string();
                        self.start_turn(prompt).await;
                    }
                }
                // scroll keys work anytime, even while busy
                KeyCode::Up => {
                    self.follow_bottom = false;
                    self.chat_scroll = self.chat_scroll.saturating_sub(1);
                }
                KeyCode::Down => {
                    self.chat_scroll = self.chat_scroll.saturating_add(1);
                    // reaching the bottom re-arms auto-follow so streamed output stays in view
                    if self.chat_scroll as usize >= self.last_max_scroll {
                        self.follow_bottom = true;
                    }
                }
                KeyCode::PageUp => {
                    self.follow_bottom = false;
                    self.chat_scroll = self.chat_scroll.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    self.chat_scroll = self.chat_scroll.saturating_add(10);
                    if self.chat_scroll as usize >= self.last_max_scroll {
                        self.follow_bottom = true;
                    }
                }
                KeyCode::End => {
                    self.follow_bottom = true;
                }
                KeyCode::Home => {
                    self.follow_bottom = false;
                    self.chat_scroll = 0;
                }
                KeyCode::Char(c) if !self.busy && !k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.input.push(c);
                }
                KeyCode::Backspace if !self.busy => {
                    self.input.pop();
                }
                _ => {}
            }
        }
        Ok(true)
    }

    async fn handle_context_viewer_key(&mut self, k: KeyEvent) -> anyhow::Result<bool> {
        let Some(viewer) = self.context_viewer.as_mut() else {
            return Ok(true);
        };
        match k.code {
            KeyCode::Esc => {
                if viewer.confirm_drop || viewer.confirm_clear {
                    viewer.confirm_drop = false;
                    viewer.confirm_clear = false;
                } else {
                    self.context_viewer = None;
                }
            }
            KeyCode::Char(c) if (c == 'n' || c == 'N') => {
                viewer.confirm_drop = false;
                viewer.confirm_clear = false;
            }
            KeyCode::Char(c) if (c == 'y' || c == 'Y') => {
                if viewer.confirm_drop {
                    if let Some(turn) = viewer.spec.turns.get(viewer.selected).cloned() {
                        let mut agent = self.agent.lock().await;
                        agent.history.drop_range(turn.msg_index, turn.msg_count);
                        drop(agent);
                        // keep the transcript consistent with the trimmed history
                        self.rebuild_messages_from_history().await;
                        self.refresh_cache().await;
                        self.rebuild_context_viewer().await;
                    }
                } else if viewer.confirm_clear {
                    let mut agent = self.agent.lock().await;
                    agent.history.clear_messages();
                    drop(agent);
                    self.rebuild_messages_from_history().await;
                    self.refresh_cache().await;
                    self.rebuild_context_viewer().await;
                }
            }
            KeyCode::Tab => {
                viewer.detail_focus = !viewer.detail_focus;
            }
            KeyCode::Up
                if !viewer.detail_focus && !viewer.confirm_drop && !viewer.confirm_clear =>
            {
                viewer.selected = viewer.selected.saturating_sub(1);
                viewer.detail_scroll = 0;
            }
            KeyCode::Down
                if !viewer.detail_focus && !viewer.confirm_drop && !viewer.confirm_clear =>
            {
                if viewer.selected + 1 < viewer.spec.turns.len() {
                    viewer.selected += 1;
                }
                viewer.detail_scroll = 0;
            }
            KeyCode::PageUp if viewer.detail_focus => {
                viewer.detail_scroll = viewer.detail_scroll.saturating_sub(10);
            }
            KeyCode::PageDown if viewer.detail_focus => {
                viewer.detail_scroll = viewer.detail_scroll.saturating_add(10);
            }
            KeyCode::Char('d')
                if !viewer.detail_focus && !viewer.confirm_drop && !viewer.confirm_clear =>
            {
                viewer.confirm_drop = true;
            }
            KeyCode::Char('c')
                if !viewer.detail_focus && !viewer.confirm_clear && !viewer.confirm_drop =>
            {
                viewer.confirm_clear = true;
            }
            _ => {}
        }
        Ok(true)
    }

    // open the interactive model picker from the discovered list.
    async fn open_model_picker(&mut self) {
        let (models, current) = {
            let agent = self.agent.lock().await;
            let current = agent.config.model.clone();
            let models = agent.config.available_models.clone();
            (models, current)
        };
        if models.is_empty() {
            self.status = "no models discovered — check the endpoint or set COM_MODEL".to_string();
            return;
        }
        let selected = models.iter().position(|m| *m == current).unwrap_or(0);
        self.model_picker = Some(ModelPickerState { models, selected, current });
    }

    async fn handle_model_picker_key(&mut self, k: KeyEvent) -> anyhow::Result<bool> {
        let Some(picker) = self.model_picker.as_mut() else {
            return Ok(true);
        };
        match k.code {
            KeyCode::Up => {
                if picker.selected > 0 {
                    picker.selected -= 1;
                }
            }
            KeyCode::Down => {
                if picker.selected + 1 < picker.models.len() {
                    picker.selected += 1;
                }
            }
            KeyCode::Esc => {
                self.model_picker = None;
                self.status = "ready".to_string();
            }
            KeyCode::Enter => {
                let chosen = picker.models[picker.selected].clone();
                self.model_picker = None;
                let mut agent = self.agent.lock().await;
                if let Some(cfg) = Arc::get_mut(&mut agent.config) {
                    cfg.model = chosen.clone();
                    self.model_name = chosen.clone();
                    self.status = format!("switched model to {chosen}");
                }
                drop(agent);
                self.refresh_cache().await;
            }
            _ => {}
        }
        Ok(true)
    }

    fn draw_model_picker(&self, f: &mut Frame<'_>, picker: &ModelPickerState) {
        let area = f.area();
        let w = 60.min(area.width.saturating_sub(4)).max(24);
        let h = (picker.models.len() as u16 + 4).min(area.height.saturating_sub(4)).max(6);
        let x = area.x + (area.width.saturating_sub(w)) / 2;
        let y = area.y + (area.height.saturating_sub(h)) / 2;
        let popup = Rect { x, y, width: w, height: h };
        let inner = (w as usize).saturating_sub(4).max(1);
        let mut items: Vec<Line<'_>> = Vec::new();
        for (i, m) in picker.models.iter().enumerate() {
            let selected = i == picker.selected;
            let current = if *m == picker.current { " (current)" } else { "" };
            let (glyph, style) = if selected {
                ("▸ ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            } else {
                ("  ", Style::default().fg(Color::White))
            };
            let text = format!("{glyph}{m}{current}");
            if text.chars().count() > inner {
                let cut: String = text.chars().take(inner.saturating_sub(1)).collect();
                items.push(Line::styled(format!("{cut}…"), style));
            } else {
                items.push(Line::styled(text, style));
            }
        }
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(Span::styled(
                " switch model — ↑↓ move · enter select · esc cancel ",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
        let list = Paragraph::new(items).block(block);
        f.render_widget(list, popup);
    }

    async fn start_turn(&mut self, prompt: String) {
        let (tx, rx) = mpsc::channel::<UiEvent>(64);
        self.ui_rx = Some(rx);
        let agent = self.agent.clone();
        let llm = self.llm.clone();
        let handle = tokio::runtime::Handle::current();

        // run the whole turn on tokio's blocking pool, never on the async
        // workers the ui loop runs on. that gives the ui a hard isolation
        // guarantee: no tool execution, blocking fs io or cpu work in a turn can
        // ever contend for a worker and stall rendering.
        tokio::task::spawn_blocking(move || {
            handle.block_on(run_turn_streamed(agent, llm, prompt, tx));
        });
    }

    async fn handle_ui_event(&mut self, evt: UiEvent) -> anyhow::Result<bool> {
        match evt {
            UiEvent::Delta(delta) => {
                self.streaming.push_str(&delta);
            }
            UiEvent::Reasoning(delta) => {
                self.reasoning.push_str(&delta);
            }
            UiEvent::RoundDone => {
                // keep reasoning bubble visible until next turn starts;
                // only flush the assistant answer here.
                if !self.streaming.is_empty() {
                    self.messages.push(TuiMessage {
                        role: Role::Assistant,
                        content: std::mem::take(&mut self.streaming),
                        command: None,
                        tool: None,
                    });
                }
            }
            UiEvent::ToolResult { name, command, preview } => {
                self.messages.push(TuiMessage {
                    role: Role::Tool,
                    command,
                    tool: Some(name),
                    content: preview,
                });
            }
            UiEvent::Context => {
                self.refresh_cache().await;
            }
            UiEvent::Status(s) => {
                self.status = s;
            }
            UiEvent::Error(e) => {
                self.messages.push(TuiMessage {
                    role: Role::Assistant,
                    content: format!("error: {e}"),
                    command: None,
                    tool: None,
                });
                self.streaming.clear();
                self.reasoning.clear();
                self.busy = false;
                self.ui_rx = None;
            }
            UiEvent::TurnDone => {
                self.busy = false;
                self.streaming.clear();
                self.reasoning.clear();
                self.ui_rx = None;
                self.refresh_cache().await;
            }
        }
        Ok(true)
    }
}

// background streaming driver — shares Agent via Arc<Mutex<Agent>>
// lock is held only during req build, history append, tool dispatch — never across .recv().await
async fn run_turn_streamed(
    agent: Arc<Mutex<Agent>>,
    llm: Arc<dyn LlmClient>,
    prompt: String,
    tx: mpsc::Sender<UiEvent>,
) {
    if let Err(e) = run_turn_inner(&agent, &llm, prompt, &tx).await {
        let _ = tx.send(UiEvent::Error(e.to_string())).await;
    }
    let _ = tx.send(UiEvent::TurnDone).await;
}

async fn run_turn_inner(
    agent: &Arc<Mutex<Agent>>,
    llm: &Arc<dyn LlmClient>,
    prompt: String,
    tx: &mpsc::Sender<UiEvent>,
) -> anyhow::Result<()> {
    {
        let mut a = agent.lock().await;
        a.history.append_user(prompt);
    }
    let _ = tx.send(UiEvent::Context).await;

    loop {
        // prune if the request would exceed the context window, then build
        // the request under lock and release the lock before streaming.
        let (req, dropped) = {
            let mut a = agent.lock().await;
            let schemas = a.tools.schemas();
            let d =
                a.history.prune_for_context(&schemas, crate::config::CONTEXT_LIMIT_TOKENS * 4 / 5);
            (a.history.to_request(&a.config.model, &schemas), d)
        };
        if dropped > 0 {
            let _ = tx
                .send(UiEvent::Status(format!("context full: dropped {dropped} old messages")))
                .await;
        }
        // stream via the shared llm client, not under the agent lock: the network
        // round trip must never block the UI from touching history/config.
        let mut rx = llm.complete_stream(req).await?;

        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut last_cost: Option<crate::llm::CallCost> = None;
        while let Some(evt) = rx.recv().await {
            match evt {
                StreamEvent::Content(delta) => {
                    content.push_str(&delta);
                    let _ = tx.send(UiEvent::Delta(delta)).await;
                }
                StreamEvent::Reasoning(delta) => {
                    reasoning.push_str(&delta);
                    let _ = tx.send(UiEvent::Reasoning(delta)).await;
                }
                StreamEvent::ToolCallStart(tc) => {
                    tool_calls.push(tc);
                }
                StreamEvent::ToolCallDelta(_) => {}
                StreamEvent::Cost(c) => {
                    last_cost = Some(c);
                }
                StreamEvent::Done => break,
                StreamEvent::Error(e) => {
                    anyhow::bail!("stream error: {e}");
                }
            }
        }

        let _ = tx.send(UiEvent::RoundDone).await;

        if let Some(c) = &last_cost {
            let _ = tx
                .send(UiEvent::Status(format!(
                    "{} cost ${:.6} (in={} out={} tok)",
                    {
                        let a = agent.lock().await;
                        a.config.model.clone()
                    },
                    c.total_cost,
                    c.input_tokens,
                    c.output_tokens
                )))
                .await;
        }

        // append assistant + dispatch tools under lock
        if tool_calls.is_empty() {
            let mut a = agent.lock().await;
            a.history.append_assistant_with_reasoning(content, reasoning, Vec::new());
            let _ = tx.send(UiEvent::Context).await;
            return Ok(());
        }
        {
            let mut a = agent.lock().await;
            a.history.append_assistant_with_reasoning(content, reasoning, tool_calls.clone());
        }
        let _ = tx.send(UiEvent::Context).await;
        for call in &tool_calls {
            // resolve the tool arc under a short lock, then run it OUTSIDE the
            // agent lock so a slow bash run never blocks the UI from touching
            // history/config or redrawing while it executes.
            let tool = {
                let a = agent.lock().await;
                a.tools.get_owned(&call.name)
            };
            let result = match tool {
                Some(t) => {
                    t.run(call.arguments.clone()).await.unwrap_or_else(|e| format!("error: {e}"))
                }
                None => format!("error: unknown tool: {}", call.name),
            };
            let preview: String = result.chars().take(200).collect();
            let command = call.arguments.get("command").and_then(|c| c.as_str()).map(String::from);
            let _ =
                tx.send(UiEvent::ToolResult { name: call.name.clone(), command, preview }).await;
            let mut a = agent.lock().await;
            a.history.append_tool_result(call.id.clone(), result);
            let _ = tx.send(UiEvent::Context).await;
        }
    }
}

// wrap `text` into lines no longer than `width` characters, never splitting a
// multi-byte char. empty lines in the source are preserved.
fn wrapped_lines(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let width = width.max(1);
    for line in text.lines() {
        if line.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut start = 0;
        let bytes = line.as_bytes();
        while start < bytes.len() {
            let mut end = (start + width).min(bytes.len());
            while end > start && !line.is_char_boundary(end) {
                end -= 1;
            }
            if end == start {
                end = (start + width).min(bytes.len());
            }
            if end <= start {
                break;
            }
            out.push(line[start..end].to_string());
            start = end;
        }
    }
    out
}

fn push_styled_wrapped(lines: &mut Vec<Line<'_>>, text: &str, style: Style, width: usize) {
    for chunk in wrapped_lines(text, width) {
        lines.push(Line::styled(chunk, style));
    }
}

// render a message with a role label, coloured body, and indented continuation.
// width is the available inner width of the chat area.
fn render_message(lines: &mut Vec<Line<'_>>, m: &TuiMessage, model_name: &str, width: usize) {
    let (colour, label) = match m.role {
        Role::User => (Color::Cyan, "you".to_string()),
        Role::Assistant => (Color::LightBlue, model_name.to_string()),
        Role::Tool => return render_tool_card(lines, m, width),
        Role::System => (Color::DarkGray, "sys".to_string()),
    };
    push_box(lines, &label, colour, &m.content, width, None);
}

// render a tool card: yellow box, with the executed command highlighted when
// one was recorded (bash tools carry it in their arguments).
fn render_tool_card(lines: &mut Vec<Line<'_>>, m: &TuiMessage, width: usize) {
    let label = m.tool.as_deref().unwrap_or("tool");
    push_box(lines, label, Color::Yellow, &m.content, width, m.command.as_deref());
}

// draw a squared-off box: top border with the label set into the left corner,
// then top padding, `│`-bordered body lines, bottom padding, and a `└──┘`
// footer. the padding keeps text off the frame edges. all border + body spans
// use `colour`; an optional `command` renders as a highlighted `$ …` line right
// below the top padding so an executed command can never be mistaken for output.
fn push_box(
    lines: &mut Vec<Line<'_>>,
    label: &str,
    colour: Color,
    body: &str,
    width: usize,
    command: Option<&str>,
) {
    let box_width = width.saturating_sub(2).max(16);
    let frame = Style::default().fg(colour);
    // squared top border with the label set into the left corner.
    let label_fill = box_width.saturating_sub(5 + label.chars().count());
    lines.push(Line::from(vec![
        Span::styled("┌─", Style::default().fg(colour)),
        Span::styled(
            format!(" {label} "),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        ),
        Span::styled("─".repeat(label_fill), Style::default().fg(colour)),
        Span::styled("┐", Style::default().fg(colour)),
    ]));

    let content_width = box_width.saturating_sub(4).max(1);
    // breathing room above and below the body so text never sits on the frame.
    lines.push(blank_row(box_width, frame));
    // command line sits right below the top padding, highlighted in a distinct
    // colour so it reads as an action, not as tool output.
    if let Some(cmd) = command {
        let cmd_style = Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD);
        let wrapped = wrapped_lines(cmd, content_width.saturating_sub(2));
        if let Some(first) = wrapped.first() {
            let pl = content_width.saturating_sub(2).saturating_sub(first.chars().count());
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(colour)),
                Span::styled("$ ", cmd_style),
                Span::styled(format!("{first}{}", " ".repeat(pl)), cmd_style),
                Span::styled(" │", Style::default().fg(colour)),
            ]));
        }
        for extra_line in wrapped.into_iter().skip(1) {
            let pl = content_width.saturating_sub(2).saturating_sub(extra_line.chars().count());
            lines.push(Line::from(vec![
                Span::styled("│ ", Style::default().fg(colour)),
                Span::styled(format!("  {extra_line}{}", " ".repeat(pl)), cmd_style),
                Span::styled(" │", Style::default().fg(colour)),
            ]));
        }
        // blank row between the command and its output.
        lines.push(blank_row(box_width, frame));
    }

    let mut body_lines: Vec<String> = if body.is_empty() {
        vec!["—".to_string()]
    } else {
        wrapped_lines(body, content_width.saturating_sub(2))
    };
    body_lines.truncate(120); // guard against runaway tool output
    for raw in body_lines {
        let pad = content_width.saturating_sub(2).saturating_sub(raw.chars().count());
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(colour)),
            Span::styled(format!("  {raw}{}", " ".repeat(pad)), Style::default().fg(colour)),
            Span::styled(" │", Style::default().fg(colour)),
        ]));
    }

    // bottom padding then a squared footer.
    lines.push(blank_row(box_width, frame));
    lines.push(Line::from(vec![
        Span::styled("└", Style::default().fg(colour)),
        Span::styled("─".repeat(box_width.saturating_sub(2)), Style::default().fg(colour)),
        Span::styled("┘", Style::default().fg(colour)),
    ]));
    lines.push(Line::from(""));
}

// a border-only row with no content, used to pad the inside of a box.
fn blank_row(box_width: usize, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled("│", style),
        Span::styled(" ".repeat(box_width.saturating_sub(2)), style),
        Span::styled("│", style),
    ])
}

fn thinking_spinner() -> char {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let frames = ['◜', '◝', '◞', '◟'];
    let idx = ((nanos / 250_000_000) as usize) % frames.len();
    frames[idx]
}

// render `path` for display, shortening the user home prefix to `~`.
fn readable_path(path: &std::path::Path) -> String {
    let s = path.display().to_string();
    if let Ok(home) = std::env::var("HOME") {
        if s == home {
            return "~".to_string();
        }
        if let Some(rest) = s.strip_prefix(&format!("{home}/")) {
            return format!("~/{rest}");
        }
    }
    s
}

fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    Ok(Terminal::new(backend)?)
}

fn restore_terminal() -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

pub async fn run(config: Config, llm: Arc<dyn LlmClient>) -> anyhow::Result<()> {
    let agent = Agent::new(config, llm);
    TuiAgent::new(agent).run().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Line;

    fn line_width(line: &Line<'_>) -> usize {
        line.spans.iter().map(|s| s.content.chars().count()).sum()
    }

    #[test]
    fn box_all_border_lines_align() {
        // every border line must be exactly the full box width; a header that
        // ends short of the right corner is the alignment regression we guard.
        for (label, command) in
            [("you", None), ("tool", Some("echo hi")), ("assistant_model", None)]
        {
            let mut lines: Vec<Line<'_>> = Vec::new();
            let width = 60usize;
            push_box(&mut lines, label, Color::Cyan, "a multi-line\nbody here", width, command);
            let expected = width.saturating_sub(2);
            assert!(!lines.is_empty());
            let (borders, sep) = lines.split_at(lines.len() - 1);
            assert_eq!(line_width(&sep[0]), 0, "box must end with a blank separator");
            for (i, l) in borders.iter().enumerate() {
                assert_eq!(line_width(l), expected, "line {i} not full box width");
            }
        }
    }
}

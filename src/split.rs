use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use scm_diff_editor::{
    DiffContext, FileContents, FileInfo, Filesystem, Opts, apply_changes, process_opts,
};
use scm_record::{FileMode, RecordState, Section, Tristate};

const ACCENT: Color = Color::Cyan;
const SELECTED: Color = Color::Yellow;
const MUTED: Color = Color::DarkGray;

pub fn run_hunk_editor(left: PathBuf, right: PathBuf) -> Result<()> {
    let filesystem = LocalFilesystem;
    let options = Opts {
        dir_diff: true,
        left,
        right,
        read_only: false,
        dry_run: false,
        base: None,
        output: None,
    };
    let DiffContext { files, write_root } =
        process_opts(&filesystem, &options).map_err(|error| anyhow::anyhow!(error))?;
    let mut picker = HunkPicker::new(RecordState {
        is_read_only: false,
        commits: Default::default(),
        files,
    });

    let accepted = picker.run()?;
    if !accepted {
        bail!("split cancelled");
    }
    if picker.entries.is_empty() || picker.selected_count() == 0 {
        bail!("select at least one hunk before splitting");
    }

    let mut filesystem = LocalFilesystem;
    apply_changes(&mut filesystem, &write_root, picker.state)
        .map_err(|error| anyhow::anyhow!(error))
}

pub fn run_description_editor(path: PathBuf) -> Result<()> {
    let original = fs::read_to_string(&path)
        .with_context(|| format!("cannot read description file {}", path.display()))?;
    let initial = original
        .lines()
        .take_while(|line| !line.starts_with("JJ:"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();
    let mut editor = DescriptionEditor { value: initial };
    if editor.run()? {
        fs::write(&path, format!("{}\n", editor.value.trim_end()))
            .with_context(|| format!("cannot write description file {}", path.display()))?;
        Ok(())
    } else {
        bail!("split description cancelled")
    }
}

struct HunkPicker {
    state: RecordState<'static>,
    entries: Vec<(usize, usize)>,
    selected: usize,
    status: String,
}

impl HunkPicker {
    fn new(state: RecordState<'static>) -> Self {
        let entries = state
            .files
            .iter()
            .enumerate()
            .flat_map(|(file_index, file)| {
                file.sections
                    .iter()
                    .enumerate()
                    .filter(|(_, section)| section.is_editable())
                    .map(move |(section_index, _)| (file_index, section_index))
            })
            .collect();
        Self {
            state,
            entries,
            selected: 0,
            status: "Space toggle hunk · c confirm split · q cancel".into(),
        }
    }

    fn run(&mut self) -> Result<bool> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let result = self.event_loop(&mut terminal);
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        result
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<bool> {
        loop {
            terminal.draw(|frame| self.draw(frame))?;
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(false),
                KeyCode::Char('c') if self.selected_count() > 0 => return Ok(true),
                KeyCode::Char('c') => self.status = "Select at least one hunk first".into(),
                KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
                KeyCode::Char('g') => self.selected = 0,
                KeyCode::Char('G') if !self.entries.is_empty() => {
                    self.selected = self.entries.len() - 1
                }
                KeyCode::Char(' ') | KeyCode::Enter => self.toggle_current(),
                KeyCode::Char('a') => self.select_all(),
                KeyCode::Char('A') => self.clear_all(),
                _ => {}
            }
        }
    }

    fn draw(&self, frame: &mut ratatui::Frame<'_>) {
        let area = frame.area();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(4),
                Constraint::Length(2),
            ])
            .split(area);
        let heading = Line::from(vec![
            Span::styled(
                " lazyjj split ",
                Style::default().fg(Color::Black).bg(ACCENT).bold(),
            ),
            Span::raw(format!(
                "  {} selected / {} hunks",
                self.selected_count(),
                self.entries.len()
            )),
        ]);
        frame.render_widget(Paragraph::new(heading), rows[0]);

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
            .split(rows[1]);
        let items = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, (file_index, section_index))| {
                let file = &self.state.files[*file_index];
                let marker = match file.sections[*section_index].tristate() {
                    Tristate::True => "[x]",
                    Tristate::Partial => "[-]",
                    Tristate::False => "[ ]",
                };
                let hunk_number = file.sections[..=*section_index]
                    .iter()
                    .filter(|section| section.is_editable())
                    .count();
                let prefix = if index == self.selected { "›" } else { " " };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{prefix} {marker} "), Style::default().fg(SELECTED)),
                    Span::styled(file.path.display().to_string(), Style::default().bold()),
                    Span::styled(format!("  hunk {hunk_number}"), Style::default().fg(MUTED)),
                ]))
            })
            .collect::<Vec<_>>();
        let list = List::new(items)
            .block(panel(" Hunks "))
            .highlight_style(Style::default().fg(SELECTED).add_modifier(Modifier::BOLD));
        let mut list_state = ListState::default().with_selected(Some(self.selected));
        frame.render_stateful_widget(list, columns[0], &mut list_state);

        let detail = self.current_hunk_text();
        frame.render_widget(
            Paragraph::new(detail)
                .block(panel(" Focused hunk "))
                .wrap(Wrap { trim: false }),
            columns[1],
        );
        frame.render_widget(
            Paragraph::new(self.status.as_str()).style(Style::default().fg(MUTED)),
            rows[2],
        );
    }

    fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = if delta.is_negative() {
            self.selected.saturating_sub(delta.unsigned_abs())
        } else {
            (self.selected + delta as usize).min(self.entries.len() - 1)
        };
    }

    fn toggle_current(&mut self) {
        let Some(&(file_index, section_index)) = self.entries.get(self.selected) else {
            self.status = "No hunks available to split".into();
            return;
        };
        self.state.files[file_index].sections[section_index].toggle_all();
        self.status = "Space toggle hunk · c confirm split · q cancel".into();
    }

    fn select_all(&mut self) {
        for (file_index, section_index) in &self.entries {
            self.state.files[*file_index].sections[*section_index].set_checked(true);
        }
    }

    fn clear_all(&mut self) {
        for (file_index, section_index) in &self.entries {
            self.state.files[*file_index].sections[*section_index].set_checked(false);
        }
    }

    fn selected_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|(file_index, section_index)| {
                self.state.files[*file_index].sections[*section_index].tristate() != Tristate::False
            })
            .count()
    }

    fn current_hunk_text(&self) -> Text<'static> {
        let Some(&(file_index, section_index)) = self.entries.get(self.selected) else {
            return Text::from("No changed hunks in selected revision");
        };
        let file = &self.state.files[file_index];
        match &file.sections[section_index] {
            Section::Changed { lines } => Text::from(
                lines
                    .iter()
                    .map(|line| {
                        let (prefix, color) = match line.change_type {
                            scm_record::ChangeType::Added => ("+", Color::Green),
                            scm_record::ChangeType::Removed => ("-", Color::Red),
                        };
                        Line::from(Span::styled(
                            format!("{prefix}{}", line.line),
                            Style::default().fg(color),
                        ))
                    })
                    .collect::<Vec<_>>(),
            ),
            Section::FileMode { mode, .. } => Text::from(format!("File mode changes to {mode}")),
            Section::Binary {
                old_description,
                new_description,
                ..
            } => Text::from(format!(
                "Binary change\nold: {}\nnew: {}",
                old_description.as_deref().unwrap_or("absent"),
                new_description.as_deref().unwrap_or("absent")
            )),
            Section::Unchanged { .. } => Text::from("No change"),
        }
    }
}

struct DescriptionEditor {
    value: String,
}

impl DescriptionEditor {
    fn run(&mut self) -> Result<bool> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let result = loop {
            terminal.draw(|frame| self.draw(frame))?;
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                continue;
            }
            match key.code {
                KeyCode::Esc => break Ok(false),
                KeyCode::F(2) => break Ok(true),
                KeyCode::Enter => self.value.push('\n'),
                KeyCode::Backspace => {
                    self.value.pop();
                }
                KeyCode::Char(character)
                    if key.modifiers.is_empty()
                        || key.modifiers == crossterm::event::KeyModifiers::SHIFT =>
                {
                    self.value.push(character);
                }
                _ => {}
            }
        };
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        result
    }

    fn draw(&self, frame: &mut ratatui::Frame<'_>) {
        let area = frame.area();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);
        frame.render_widget(
            Paragraph::new("lazyjj split description\nEnter description for this split revision, then press F2 to confirm.")
                .style(Style::default().fg(ACCENT)),
            rows[0],
        );
        frame.render_widget(
            Paragraph::new(format!("{}█", self.value))
                .block(panel(" Description "))
                .wrap(Wrap { trim: false }),
            rows[1],
        );
        frame.render_widget(
            Paragraph::new("Enter newline · F2 confirm · Esc cancel")
                .style(Style::default().fg(MUTED)),
            rows[2],
        );
    }
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(ACCENT))
}

struct LocalFilesystem;

impl Filesystem for LocalFilesystem {
    fn read_dir_diff_paths(
        &self,
        left: &Path,
        right: &Path,
    ) -> scm_diff_editor::Result<BTreeSet<PathBuf>> {
        fn collect(
            root: &Path,
            current: &Path,
            paths: &mut BTreeSet<PathBuf>,
        ) -> scm_diff_editor::Result<()> {
            let entries = match fs::read_dir(current) {
                Ok(entries) => entries,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => {
                    return Err(scm_diff_editor::Error::ReadFile {
                        path: current.to_owned(),
                        source: error,
                    });
                }
            };
            for entry in entries {
                let entry = entry.map_err(|error| scm_diff_editor::Error::ReadFile {
                    path: current.to_owned(),
                    source: error,
                })?;
                let path = entry.path();
                let file_type =
                    entry
                        .file_type()
                        .map_err(|error| scm_diff_editor::Error::ReadFile {
                            path: path.clone(),
                            source: error,
                        })?;
                if file_type.is_dir() {
                    collect(root, &path, paths)?;
                } else if file_type.is_file() || file_type.is_symlink() {
                    paths.insert(
                        path.strip_prefix(root)
                            .expect("walked beneath root")
                            .to_owned(),
                    );
                }
            }
            Ok(())
        }

        let mut paths = BTreeSet::new();
        collect(left, left, &mut paths)?;
        collect(right, right, &mut paths)?;
        Ok(paths)
    }

    fn read_file_info(&self, path: &Path) -> scm_diff_editor::Result<FileInfo> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(FileInfo {
                    file_mode: FileMode::Absent,
                    contents: FileContents::Absent,
                });
            }
            Err(error) => {
                return Err(scm_diff_editor::Error::ReadFile {
                    path: path.to_owned(),
                    source: error,
                });
            }
        };
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o001 == 0o001 {
                FileMode::Unix(0o100755)
            } else {
                FileMode::Unix(0o100644)
            }
        };
        #[cfg(not(unix))]
        let mode = FileMode::Unix(0o100644);
        let bytes = fs::read(path).map_err(|error| scm_diff_editor::Error::ReadFile {
            path: path.to_owned(),
            source: error,
        })?;
        let contents = match String::from_utf8(bytes.clone()) {
            Ok(contents) if !bytes.contains(&0) => FileContents::Text {
                contents,
                hash: String::new(),
                num_bytes: bytes.len() as u64,
            },
            _ => FileContents::Binary {
                hash: "binary".into(),
                num_bytes: bytes.len() as u64,
            },
        };
        Ok(FileInfo {
            file_mode: mode,
            contents,
        })
    }

    fn write_file(&mut self, path: &Path, contents: &str) -> scm_diff_editor::Result<()> {
        fs::write(path, contents).map_err(|error| scm_diff_editor::Error::WriteFile {
            path: path.to_owned(),
            source: error,
        })
    }

    fn copy_file(&mut self, old_path: &Path, new_path: &Path) -> scm_diff_editor::Result<()> {
        fs::copy(old_path, new_path)
            .map(|_| ())
            .map_err(|error| scm_diff_editor::Error::CopyFile {
                old_path: old_path.to_owned(),
                new_path: new_path.to_owned(),
                source: error,
            })
    }

    fn remove_file(&mut self, path: &Path) -> scm_diff_editor::Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(scm_diff_editor::Error::RemoveFile {
                path: path.to_owned(),
                source: error,
            }),
        }
    }

    fn create_dir_all(&mut self, path: &Path) -> scm_diff_editor::Result<()> {
        fs::create_dir_all(path).map_err(|error| scm_diff_editor::Error::CreateDirAll {
            path: path.to_owned(),
            source: error,
        })
    }
}

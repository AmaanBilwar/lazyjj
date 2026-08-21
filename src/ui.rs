use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{App, Focus, Overlay};

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;
const SELECTED: Color = Color::Yellow;
const DANGER: Color = Color::Red;

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let page = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());

    draw_header(frame, app, page[0]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(54), Constraint::Percentage(46)])
        .split(page[1]);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(columns[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(4)])
        .split(columns[1]);

    draw_revisions(frame, app, left[0]);
    draw_bookmarks(frame, app, left[1]);
    draw_details(frame, app, right[0]);
    draw_changes(frame, app, right[1]);
    draw_footer(frame, app, page[2]);

    if let Some(overlay) = &app.overlay {
        draw_overlay(frame, overlay);
    }
}

fn draw_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let root = app.root.as_deref().unwrap_or("Not inside a jj repository");
    let header = Line::from(vec![
        Span::styled(
            " lazyjj ",
            Style::default().fg(Color::Black).bg(ACCENT).bold(),
        ),
        Span::raw("  "),
        Span::styled(root, Style::default().fg(Color::White)),
    ]);
    frame.render_widget(Paragraph::new(header), area);
}

fn draw_revisions(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let items = app
        .revisions
        .iter()
        .map(|revision| {
            let marker = if revision.is_working_copy {
                "●"
            } else {
                "○"
            };
            let description = if revision.description.is_empty() {
                "(no description)"
            } else {
                &revision.description
            };
            let bookmark = if revision.bookmarks.is_empty() {
                String::new()
            } else {
                format!("  {}", revision.bookmarks)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{marker} "), Style::default().fg(ACCENT)),
                Span::styled(&revision.change_id, Style::default().fg(Color::Blue)),
                Span::styled(bookmark, Style::default().fg(Color::Magenta)),
                Span::raw(format!("  {description}")),
            ]))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(panel(" 1 Revisions ", app.focus == Focus::Revisions))
        .highlight_symbol("› ")
        .highlight_style(Style::default().fg(SELECTED).add_modifier(Modifier::BOLD));
    let mut state = ListState::default().with_selected(Some(app.revision_index));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_bookmarks(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let items = app
        .bookmarks
        .iter()
        .map(|bookmark| {
            let (marker, scope, color) = match (&bookmark.remote, bookmark.tracked) {
                (Some(_), true) => ("◇ ", "tracked", Color::Green),
                (Some(_), false) => ("◇ ", "untracked", Color::Yellow),
                (None, _) => ("◆ ", "local", Color::Magenta),
            };
            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(color)),
                Span::styled(bookmark.symbol(), Style::default().bold()),
                Span::styled(format!("  {scope}"), Style::default().fg(color)),
                Span::styled(
                    format!("  {}", bookmark.change_id),
                    Style::default().fg(MUTED),
                ),
            ]))
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(panel(" 2 Bookmarks ", app.focus == Focus::Bookmarks))
        .highlight_symbol("› ")
        .highlight_style(Style::default().fg(SELECTED).add_modifier(Modifier::BOLD));
    let mut state = ListState::default().with_selected(Some(app.bookmark_index));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_details(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let text = if let Some(revision) = app.selected_revision() {
        Text::from(vec![
            Line::from(vec![
                Span::styled("Change  ", Style::default().fg(MUTED)),
                Span::styled(&revision.change_id, Style::default().fg(ACCENT)),
            ]),
            Line::from(vec![
                Span::styled("Commit  ", Style::default().fg(MUTED)),
                Span::raw(&revision.commit_id),
            ]),
            Line::from(vec![
                Span::styled("Marks   ", Style::default().fg(MUTED)),
                Span::styled(&revision.bookmarks, Style::default().fg(Color::Magenta)),
            ]),
            Line::from(""),
            Line::from(if revision.description.is_empty() {
                "(no description)"
            } else {
                &revision.description
            }),
        ])
    } else {
        Text::from("No revision selected")
    };
    frame.render_widget(
        Paragraph::new(text)
            .block(panel(" Details ", false))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_changes(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let items = if app.changed_files.is_empty() {
        vec![ListItem::new(Span::styled(
            "No changed files",
            Style::default().fg(MUTED),
        ))]
    } else {
        app.changed_files
            .iter()
            .map(|file| ListItem::new(file.as_str()))
            .collect()
    };
    frame.render_widget(
        List::new(items).block(panel(" Changed files ", false)),
        area,
    );
}

fn draw_footer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(2)])
        .split(area);
    let status_style = if app.status_is_error {
        Style::default().fg(DANGER)
    } else {
        Style::default().fg(MUTED)
    };
    frame.render_widget(
        Paragraph::new(app.status.as_str()).style(status_style),
        rows[0],
    );

    let keys = Line::from(vec![
        key("1/2", "pane"),
        key("j/k", "move"),
        key("e", "describe"),
        key("n", "new mark"),
        key("m", "move mark"),
        key("-", "move to @-"),
        key("p", "push"),
        key("t", "track"),
        key("u", "undo"),
        key("?", "help"),
        key("q", "quit"),
    ]);
    frame.render_widget(Paragraph::new(keys).wrap(Wrap { trim: false }), rows[1]);
}

fn key<'a>(binding: &'a str, label: &'a str) -> Span<'a> {
    Span::styled(
        format!(" {binding} {label} "),
        Style::default().fg(Color::Black).bg(Color::Gray),
    )
}

fn panel(title: &'static str, focused: bool) -> Block<'static> {
    let border = if focused { ACCENT } else { MUTED };
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(border))
}

fn draw_overlay(frame: &mut Frame<'_>, overlay: &Overlay) {
    match overlay {
        Overlay::Help => {
            let area = centered(frame.area(), 64, 20);
            let help = Text::from(vec![
                Line::styled("Navigation", Style::default().fg(ACCENT).bold()),
                Line::from("  1 / 2               focus numbered pane"),
                Line::from("  Tab / Shift+Tab     cycle panes"),
                Line::from("  j / k or arrows     select item"),
                Line::from(""),
                Line::styled("Actions", Style::default().fg(ACCENT).bold()),
                Line::from("  e   edit selected revision description"),
                Line::from("  n   create bookmark on selected revision"),
                Line::from("  m   move selected bookmark to selected revision"),
                Line::from("  -   move selected bookmark to @-"),
                Line::from("  p   push selected local bookmark"),
                Line::from("  t   track selected remote bookmark"),
                Line::from("  d   delete selected local bookmark"),
                Line::from("  u   undo latest jj operation"),
                Line::from("  r   refresh repository"),
                Line::from(""),
                Line::styled("Esc or ? closes help", Style::default().fg(MUTED)),
            ]);
            draw_popup(frame, area, " Help ", Paragraph::new(help));
        }
        Overlay::BookmarkInput { value } => {
            let area = centered(frame.area(), 60, 7);
            let text = Text::from(vec![
                Line::from("Create bookmark on selected revision"),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Name: ", Style::default().fg(MUTED)),
                    Span::styled(format!("{value}█"), Style::default().fg(SELECTED)),
                ]),
                Line::from(""),
                Line::styled("Enter continue · Esc cancel", Style::default().fg(MUTED)),
            ]);
            draw_popup(frame, area, " New bookmark ", Paragraph::new(text));
        }
        Overlay::DescriptionInput { value } => {
            let area = centered(frame.area(), 72, 8);
            let text = Text::from(vec![
                Line::from("Describe selected revision"),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Description: ", Style::default().fg(MUTED)),
                    Span::styled(format!("{value}█"), Style::default().fg(SELECTED)),
                ]),
                Line::from(""),
                Line::styled(
                    "Enter continue · Esc cancel · empty text clears description",
                    Style::default().fg(MUTED),
                ),
            ]);
            draw_popup(frame, area, " Describe ", Paragraph::new(text));
        }
        Overlay::Confirm(command) => {
            let area = centered(frame.area(), 72, 9);
            let text = Text::from(vec![
                Line::styled(&command.label, Style::default().bold()),
                Line::from(""),
                Line::styled(command.display(), Style::default().fg(SELECTED)),
                Line::from(""),
                Line::styled(
                    "Run command? Enter/y confirm · n/Esc cancel",
                    Style::default().fg(MUTED),
                ),
            ]);
            draw_popup(
                frame,
                area,
                " Confirm ",
                Paragraph::new(text).wrap(Wrap { trim: false }),
            );
        }
    }
}

fn draw_popup<'a>(frame: &mut Frame<'_>, area: Rect, title: &'a str, content: Paragraph<'a>) {
    frame.render_widget(Clear, area);
    frame.render_widget(
        content
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(ACCENT)),
            )
            .alignment(Alignment::Left),
        area,
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width.saturating_sub(width)) / 2),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1])[1]
}

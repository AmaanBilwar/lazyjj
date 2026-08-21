mod app;
mod jj;
mod split;
mod ui;

use std::{env, io, path::PathBuf, process::Command, time::Duration};

use anyhow::Result;
use app::{App, PendingCommand};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

fn main() -> Result<()> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    match arguments.next().as_deref() {
        Some(command) if command == "hunk-editor" => {
            let left = arguments
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("missing left split directory"))?;
            let right = arguments
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("missing right split directory"))?;
            return split::run_hunk_editor(left, right);
        }
        Some(command) if command == "description-editor" => {
            let path = arguments
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("missing Jujutsu description file"))?;
            return split::run_description_editor(path);
        }
        Some(command) => anyhow::bail!("unknown lazyjj command: {:?}", command),
        None => {}
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();

    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            app.should_quit = true;
        } else {
            app.handle_key(key);
        }

        if let Some(command) = app.take_interactive_command() {
            run_interactive_command(terminal, &mut app, command)?;
        }
    }

    Ok(())
}

fn run_interactive_command(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    command: PendingCommand,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let command_result = Command::new("jj").args(&command.args).status();

    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    terminal.clear()?;

    let result = match command_result {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("jj exited with {status}")),
        Err(error) => Err(format!("failed to start jj: {error}")),
    };
    app.finish_interactive_command(&command, result);
    Ok(())
}

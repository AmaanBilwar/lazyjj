use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::jj::{Bookmark, Jj, Revision};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Focus {
    Revisions,
    Bookmarks,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Revisions => Self::Bookmarks,
            Self::Bookmarks => Self::Revisions,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PendingCommand {
    pub label: String,
    pub args: Vec<String>,
}

impl PendingCommand {
    pub fn display(&self) -> String {
        format!("jj {}", self.args.join(" "))
    }
}

#[derive(Clone, Debug)]
pub enum Overlay {
    Help,
    BookmarkInput { value: String },
    DescriptionInput { value: String },
    Confirm(PendingCommand),
}

pub struct App {
    pub should_quit: bool,
    pub root: Option<String>,
    pub revisions: Vec<Revision>,
    pub bookmarks: Vec<Bookmark>,
    pub changed_files: Vec<String>,
    pub revision_index: usize,
    pub bookmark_index: usize,
    pub focus: Focus,
    pub overlay: Option<Overlay>,
    pub status: String,
    pub status_is_error: bool,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            should_quit: false,
            root: None,
            revisions: Vec::new(),
            bookmarks: Vec::new(),
            changed_files: Vec::new(),
            revision_index: 0,
            bookmark_index: 0,
            focus: Focus::Revisions,
            overlay: None,
            status: "Loading repository…".into(),
            status_is_error: false,
        };
        app.refresh();
        app
    }

    pub fn selected_revision(&self) -> Option<&Revision> {
        self.revisions.get(self.revision_index)
    }

    pub fn selected_bookmark(&self) -> Option<&Bookmark> {
        self.bookmarks.get(self.bookmark_index)
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.overlay.is_some() {
            self.handle_overlay_key(key);
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.overlay = Some(Overlay::Help),
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Tab | KeyCode::BackTab => self.focus = self.focus.next(),
            KeyCode::Char('1') => self.focus = Focus::Revisions,
            KeyCode::Char('2') => self.focus = Focus::Bookmarks,
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
            KeyCode::Char('e') => self.edit_description(),
            KeyCode::Char('n') => self.create_bookmark(),
            KeyCode::Char('m') => self.move_bookmark_to_selection(),
            KeyCode::Char('-') => self.move_bookmark_to_parent(),
            KeyCode::Char('p') => self.push_bookmark(),
            KeyCode::Char('t') => self.track_bookmark(),
            KeyCode::Char('d') => self.delete_bookmark(),
            KeyCode::Char('u') => {
                self.confirm("Undo latest repository operation", vec!["undo".into()])
            }
            _ => {}
        }
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) {
        let Some(overlay) = self.overlay.take() else {
            return;
        };

        match overlay {
            Overlay::Help => match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {}
                _ => self.overlay = Some(Overlay::Help),
            },
            Overlay::BookmarkInput { mut value } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter if value.trim().is_empty() => {
                    self.set_error("Bookmark name cannot be empty");
                }
                KeyCode::Enter => {
                    let Some(revision) = self.selected_revision() else {
                        self.set_error("No revision selected");
                        return;
                    };
                    self.confirm(
                        "Create bookmark",
                        vec![
                            "bookmark".into(),
                            "create".into(),
                            value.trim().into(),
                            "-r".into(),
                            revision.change_id.clone(),
                        ],
                    );
                }
                KeyCode::Backspace => {
                    value.pop();
                    self.overlay = Some(Overlay::BookmarkInput { value });
                }
                KeyCode::Char(character)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    value.push(character);
                    self.overlay = Some(Overlay::BookmarkInput { value });
                }
                _ => self.overlay = Some(Overlay::BookmarkInput { value }),
            },
            Overlay::DescriptionInput { mut value } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    let Some(revision) = self.selected_revision() else {
                        self.set_error("No revision selected");
                        return;
                    };
                    self.confirm(
                        "Update selected revision description",
                        vec![
                            "describe".into(),
                            "-r".into(),
                            revision.change_id.clone(),
                            "-m".into(),
                            value,
                        ],
                    );
                }
                KeyCode::Backspace => {
                    value.pop();
                    self.overlay = Some(Overlay::DescriptionInput { value });
                }
                KeyCode::Char(character)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    value.push(character);
                    self.overlay = Some(Overlay::DescriptionInput { value });
                }
                _ => self.overlay = Some(Overlay::DescriptionInput { value }),
            },
            Overlay::Confirm(command) => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => self.execute(command),
                KeyCode::Char('n') | KeyCode::Esc => {}
                _ => self.overlay = Some(Overlay::Confirm(command)),
            },
        }
    }

    fn select_next(&mut self) {
        match self.focus {
            Focus::Revisions if !self.revisions.is_empty() => {
                self.revision_index = (self.revision_index + 1).min(self.revisions.len() - 1);
                self.load_changed_files();
            }
            Focus::Bookmarks if !self.bookmarks.is_empty() => {
                self.bookmark_index = (self.bookmark_index + 1).min(self.bookmarks.len() - 1);
            }
            _ => {}
        }
    }

    fn select_previous(&mut self) {
        match self.focus {
            Focus::Revisions => {
                self.revision_index = self.revision_index.saturating_sub(1);
                self.load_changed_files();
            }
            Focus::Bookmarks => {
                self.bookmark_index = self.bookmark_index.saturating_sub(1);
            }
        }
    }

    fn edit_description(&mut self) {
        let Some(revision) = self.selected_revision() else {
            self.set_error("No revision selected");
            return;
        };
        self.overlay = Some(Overlay::DescriptionInput {
            value: revision.description.clone(),
        });
    }

    fn create_bookmark(&mut self) {
        if self.selected_revision().is_none() {
            self.set_error("No revision selected");
            return;
        }
        self.overlay = Some(Overlay::BookmarkInput {
            value: String::new(),
        });
    }

    fn move_bookmark_to_selection(&mut self) {
        let Some(bookmark) = self.selected_bookmark() else {
            self.set_error("No bookmark selected");
            return;
        };
        if bookmark.is_remote() {
            self.set_error("Only local bookmarks can be moved");
            return;
        }
        let Some(revision) = self.selected_revision() else {
            self.set_error("No revision selected");
            return;
        };
        self.confirm(
            "Move selected bookmark to selected revision",
            vec![
                "bookmark".into(),
                "move".into(),
                bookmark.name.clone(),
                "--to".into(),
                revision.change_id.clone(),
                "--allow-backwards".into(),
            ],
        );
    }

    fn move_bookmark_to_parent(&mut self) {
        let Some(bookmark) = self.selected_bookmark() else {
            self.set_error("No bookmark selected");
            return;
        };
        if bookmark.is_remote() {
            self.set_error("Only local bookmarks can be moved");
            return;
        }
        self.confirm(
            "Move selected bookmark to @-",
            vec![
                "bookmark".into(),
                "move".into(),
                bookmark.name.clone(),
                "--to".into(),
                "@-".into(),
                "--allow-backwards".into(),
            ],
        );
    }

    fn push_bookmark(&mut self) {
        let Some(bookmark) = self.selected_bookmark() else {
            self.set_error("No bookmark selected");
            return;
        };
        if bookmark.is_remote() {
            self.set_error("Only local bookmarks can be pushed");
            return;
        }
        self.confirm(
            "Push selected bookmark",
            vec![
                "git".into(),
                "push".into(),
                "--bookmark".into(),
                bookmark.name.clone(),
            ],
        );
    }

    fn track_bookmark(&mut self) {
        let Some(bookmark) = self.selected_bookmark() else {
            self.set_error("No bookmark selected");
            return;
        };
        if !bookmark.is_remote() {
            self.set_error("Select a remote bookmark to track");
            return;
        }
        if bookmark.tracked {
            self.set_error(format!("{} is already tracked", bookmark.symbol()));
            return;
        }
        self.confirm(
            "Track selected remote bookmark",
            vec!["bookmark".into(), "track".into(), bookmark.symbol()],
        );
    }

    fn delete_bookmark(&mut self) {
        let Some(bookmark) = self.selected_bookmark() else {
            self.set_error("No bookmark selected");
            return;
        };
        if bookmark.is_remote() {
            self.set_error("Only local bookmarks can be deleted");
            return;
        }
        self.confirm(
            "Delete selected local bookmark",
            vec!["bookmark".into(), "delete".into(), bookmark.name.clone()],
        );
    }

    fn confirm(&mut self, label: impl Into<String>, args: Vec<String>) {
        self.overlay = Some(Overlay::Confirm(PendingCommand {
            label: label.into(),
            args,
        }));
    }

    fn execute(&mut self, command: PendingCommand) {
        let display = command.display();
        match Jj::run(&command.args) {
            Ok(_) => {
                self.refresh();
                self.status = format!("Completed: {display}");
                self.status_is_error = false;
            }
            Err(error) => self.set_error(format!("{display}: {error:#}")),
        }
    }

    fn refresh(&mut self) {
        match Jj::load() {
            Ok(snapshot) => {
                self.root = Some(snapshot.root.display().to_string());
                self.revisions = snapshot.revisions;
                self.bookmarks = snapshot.bookmarks;
                self.revision_index = self
                    .revision_index
                    .min(self.revisions.len().saturating_sub(1));
                self.bookmark_index = self
                    .bookmark_index
                    .min(self.bookmarks.len().saturating_sub(1));
                self.load_changed_files();
                self.status = format!(
                    "Loaded {} revisions and {} bookmarks",
                    self.revisions.len(),
                    self.bookmarks.len()
                );
                self.status_is_error = false;
            }
            Err(error) => {
                self.root = None;
                self.revisions.clear();
                self.bookmarks.clear();
                self.changed_files.clear();
                self.set_error(error.to_string());
            }
        }
    }

    fn load_changed_files(&mut self) {
        let Some(revision) = self.selected_revision() else {
            self.changed_files.clear();
            return;
        };
        let change_id = revision.change_id.clone();
        match Jj::changed_files(&change_id) {
            Ok(files) => self.changed_files = files,
            Err(error) => {
                self.changed_files.clear();
                self.set_error(format!("Cannot load diff for {change_id}: {error:#}"));
            }
        }
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_is_error = true;
    }
}

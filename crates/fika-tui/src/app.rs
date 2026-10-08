use std::time::Duration;

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use fika_station::Station;

use crate::line_edit::{Action, LineEditor};

pub struct App {
    pub station: Station,
    pub editor: LineEditor,
    pub status: String,
    pub show_help: bool,
    quit: bool,
}

impl App {
    pub fn new(station: Station) -> Self {
        Self {
            station,
            editor: LineEditor::default(),
            status: "type a message and press Enter, /help for commands".into(),
            show_help: false,
            quit: false,
        }
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while !self.quit {
            self.station.poll();
            terminal.draw(|f| crate::ui::draw(f, &self))?;
            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.on_key(key.code, key.modifiers);
            }
        }
        Ok(())
    }

    fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        match code {
            KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
                self.quit = true;
                return;
            }
            KeyCode::Char('l') if mods.contains(KeyModifiers::CONTROL) => {
                self.station.chat.clear();
                return;
            }
            KeyCode::Esc | KeyCode::Char('g')
                if code == KeyCode::Esc || mods.contains(KeyModifiers::CONTROL) =>
            {
                if self.show_help && code == KeyCode::Esc {
                    self.show_help = false;
                } else if self.station.abort_tx() {
                    self.status = "transmission aborted".into();
                } else {
                    self.status = "nothing to abort".into();
                }
                return;
            }
            KeyCode::F(1) => {
                self.show_help = !self.show_help;
                return;
            }
            _ => {}
        }
        if let Action::Submit(line) = self.editor.key(code, mods) {
            self.submit(line.trim());
        }
    }

    fn submit(&mut self, line: &str) {
        if line.is_empty() {
            return;
        }
        if let Some(cmd) = line.strip_prefix('/') {
            self.command(cmd);
            return;
        }
        match self.station.send_text(line) {
            Ok(()) => self.status = format!("queued for {}", self.station.dest_label),
            Err(e) => self.status = format!("not sent: {e}"),
        }
    }

    fn command(&mut self, cmd: &str) {
        let mut parts = cmd.splitn(2, ' ');
        let name = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();
        match name {
            "quit" | "q" | "exit" => self.quit = true,
            "help" | "h" | "?" => self.show_help = !self.show_help,
            "to" if !arg.is_empty() => {
                self.station.set_dest(arg);
                self.status = format!("destination {}", self.station.dest_label);
            }
            "profile" | "speed" => match arg.parse::<fika_modem::Profile>() {
                Ok(p) => {
                    self.station.profile = p;
                    self.status = format!("profile {p}");
                }
                Err(e) => self.status = e,
            },
            "beacon" => match self.station.send_beacon() {
                Ok(()) => self.status = "beacon queued".into(),
                Err(e) => self.status = format!("beacon failed: {e}"),
            },
            "clear" => {
                self.station.chat.clear();
                self.status = "chat cleared".into();
            }
            _ => self.status = format!("unknown command /{name}; /help lists commands"),
        }
    }
}

use std::time::Duration;

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use fika_station::Station;

pub struct App {
    pub station: Station,
    pub input: String,
    pub cursor: usize,
    pub status: String,
    pub show_help: bool,
    quit: bool,
}

impl App {
    pub fn new(station: Station) -> Self {
        Self {
            station,
            input: String::new(),
            cursor: 0,
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
            KeyCode::Char('c') | KeyCode::Char('d') if mods.contains(KeyModifiers::CONTROL) => {
                self.quit = true
            }
            KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                self.cursor = 0;
            }
            KeyCode::Char(c) => {
                self.input.insert(self.byte_index(), c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let i = self.byte_index();
                self.input.remove(i);
            }
            KeyCode::Delete if self.cursor < self.input.chars().count() => {
                let i = self.byte_index();
                self.input.remove(i);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.input.chars().count()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.chars().count(),
            KeyCode::Esc => self.show_help = false,
            KeyCode::F(1) => self.show_help = !self.show_help,
            KeyCode::Enter => {
                let line = std::mem::take(&mut self.input);
                self.cursor = 0;
                self.submit(line.trim());
            }
            _ => {}
        }
    }

    fn byte_index(&self) -> usize {
        self.input
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
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
            "lane" => match arg.parse::<usize>() {
                Ok(l) if l < 4 => {
                    self.station.lane = l;
                    self.status = format!("transmit lane {l}");
                }
                _ => self.status = "usage: /lane 0..3".into(),
            },
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

//! Single-line editor with Emacs keybindings, a kill buffer and history.

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

#[derive(Default)]
pub struct LineEditor {
    chars: Vec<char>,
    cursor: usize,
    kill: String,
    history: Vec<String>,
    /// Index into history while browsing, with the draft saved aside.
    browse: Option<(usize, Vec<char>)>,
}

/// What a key press did, for the caller.
pub enum Action {
    None,
    /// Enter: the line was submitted and the editor cleared.
    Submit(String),
}

impl LineEditor {
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn set(&mut self, s: &str) {
        self.chars = s.chars().collect();
        self.cursor = self.chars.len();
    }

    fn is_word(c: char) -> bool {
        c.is_alphanumeric()
    }

    /// Start of the word before the cursor.
    fn word_start(&self) -> usize {
        let mut i = self.cursor;
        while i > 0 && !Self::is_word(self.chars[i - 1]) {
            i -= 1;
        }
        while i > 0 && Self::is_word(self.chars[i - 1]) {
            i -= 1;
        }
        i
    }

    /// End of the word after the cursor.
    fn word_end(&self) -> usize {
        let n = self.chars.len();
        let mut i = self.cursor;
        while i < n && !Self::is_word(self.chars[i]) {
            i += 1;
        }
        while i < n && Self::is_word(self.chars[i]) {
            i += 1;
        }
        i
    }

    fn kill_range(&mut self, a: usize, b: usize) {
        if a >= b {
            return;
        }
        self.kill = self.chars[a..b].iter().collect();
        self.chars.drain(a..b);
        self.cursor = a;
    }

    fn insert(&mut self, c: char) {
        self.chars.insert(self.cursor, c);
        self.cursor += 1;
    }

    fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let (idx, draft) = match self.browse.take() {
            None => (self.history.len() - 1, self.chars.clone()),
            Some((0, draft)) => (0, draft),
            Some((i, draft)) => (i - 1, draft),
        };
        self.chars = self.history[idx].chars().collect();
        self.cursor = self.chars.len();
        self.browse = Some((idx, draft));
    }

    fn history_down(&mut self) {
        match self.browse.take() {
            None => {}
            Some((i, draft)) if i + 1 >= self.history.len() => {
                self.chars = draft;
                self.cursor = self.chars.len();
            }
            Some((i, draft)) => {
                self.chars = self.history[i + 1].chars().collect();
                self.cursor = self.chars.len();
                self.browse = Some((i + 1, draft));
            }
        }
    }

    pub fn key(&mut self, code: KeyCode, mods: KeyModifiers) -> Action {
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        let alt = mods.contains(KeyModifiers::ALT);
        let n = self.chars.len();
        match (code, ctrl, alt) {
            // Movement.
            (KeyCode::Char('a'), true, _) | (KeyCode::Home, _, _) => self.cursor = 0,
            (KeyCode::Char('e'), true, _) | (KeyCode::End, _, _) => self.cursor = n,
            (KeyCode::Char('b'), true, _) | (KeyCode::Left, false, false) => {
                self.cursor = self.cursor.saturating_sub(1)
            }
            (KeyCode::Char('f'), true, _) | (KeyCode::Right, false, false) => {
                self.cursor = (self.cursor + 1).min(n)
            }
            (KeyCode::Char('b'), _, true) | (KeyCode::Left, true, _) | (KeyCode::Left, _, true) => {
                self.cursor = self.word_start()
            }
            (KeyCode::Char('f'), _, true)
            | (KeyCode::Right, true, _)
            | (KeyCode::Right, _, true) => self.cursor = self.word_end(),
            // Deleting.
            (KeyCode::Char('h'), true, _) | (KeyCode::Backspace, false, false) => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.chars.remove(self.cursor);
                }
            }
            (KeyCode::Char('d'), true, _) | (KeyCode::Delete, _, _) => {
                if self.cursor < n {
                    self.chars.remove(self.cursor);
                }
            }
            // Killing (into the kill buffer).
            (KeyCode::Char('k'), true, _) => self.kill_range(self.cursor, n),
            (KeyCode::Char('u'), true, _) => self.kill_range(0, self.cursor),
            (KeyCode::Char('w'), true, _)
            | (KeyCode::Backspace, _, true)
            | (KeyCode::Backspace, true, _) => {
                let a = self.word_start();
                self.kill_range(a, self.cursor);
            }
            (KeyCode::Char('d'), _, true) => {
                let b = self.word_end();
                self.kill_range(self.cursor, b);
            }
            (KeyCode::Char('y'), true, _) => {
                let k: Vec<char> = self.kill.chars().collect();
                for c in k {
                    self.insert(c);
                }
            }
            (KeyCode::Char('t'), true, _) => {
                // Transpose the two characters before the cursor (at end) or around it.
                if n >= 2 {
                    let i = self.cursor.clamp(1, n - 1);
                    self.chars.swap(i - 1, i);
                    self.cursor = (i + 1).min(n);
                }
            }
            // History.
            (KeyCode::Char('p'), true, _) | (KeyCode::Up, _, _) => self.history_up(),
            (KeyCode::Char('n'), true, _) | (KeyCode::Down, _, _) => self.history_down(),
            // Submit.
            (KeyCode::Enter, _, _) => {
                let line = self.text();
                self.chars.clear();
                self.cursor = 0;
                self.browse = None;
                if !line.trim().is_empty() && self.history.last() != Some(&line) {
                    self.history.push(line.clone());
                }
                return Action::Submit(line);
            }
            // Plain characters (Shift is fine; Ctrl/Alt combos above are consumed).
            (KeyCode::Char(c), false, false) => self.insert(c),
            _ => {}
        }
        Action::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(e: &mut LineEditor, keys: &[(KeyCode, KeyModifiers)]) {
        for &(c, m) in keys {
            e.key(c, m);
        }
    }

    fn ctrl(c: char) -> (KeyCode, KeyModifiers) {
        (KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn alt(c: char) -> (KeyCode, KeyModifiers) {
        (KeyCode::Char(c), KeyModifiers::ALT)
    }

    #[test]
    fn movement_and_kill_yank() {
        let mut e = LineEditor::default();
        e.set("hej på dig");
        press(&mut e, &[ctrl('a')]);
        assert_eq!(e.cursor(), 0);
        press(&mut e, &[alt('f')]);
        assert_eq!(e.cursor(), 3);
        press(&mut e, &[ctrl('k')]);
        assert_eq!(e.text(), "hej");
        press(&mut e, &[ctrl('y')]);
        assert_eq!(e.text(), "hej på dig");
        press(&mut e, &[ctrl('e'), ctrl('w')]);
        assert_eq!(e.text(), "hej på ");
        press(&mut e, &[ctrl('u')]);
        assert_eq!(e.text(), "");
        press(&mut e, &[ctrl('y')]);
        assert_eq!(e.text(), "hej på ");
    }

    #[test]
    fn delete_backspace_and_transpose() {
        let mut e = LineEditor::default();
        e.set("abc");
        press(&mut e, &[ctrl('h')]);
        assert_eq!(e.text(), "ab");
        press(&mut e, &[ctrl('b'), ctrl('d')]);
        assert_eq!(e.text(), "a");
        e.set("ab");
        press(&mut e, &[ctrl('t')]);
        assert_eq!(e.text(), "ba");
    }

    #[test]
    fn history_browsing_keeps_draft() {
        let mut e = LineEditor::default();
        e.set("first");
        assert!(
            matches!(e.key(KeyCode::Enter, KeyModifiers::NONE), Action::Submit(s) if s == "first")
        );
        e.set("second");
        e.key(KeyCode::Enter, KeyModifiers::NONE);
        e.set("dra");
        press(&mut e, &[ctrl('p')]);
        assert_eq!(e.text(), "second");
        press(&mut e, &[ctrl('p')]);
        assert_eq!(e.text(), "first");
        press(&mut e, &[ctrl('n'), ctrl('n')]);
        assert_eq!(e.text(), "dra");
    }
}

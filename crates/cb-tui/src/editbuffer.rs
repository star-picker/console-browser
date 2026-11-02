//! 单行编辑器：地址栏 / 查找 / 命令行共用。
//!
//! 不引第三方输入框，行为可测、可预期。

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditBuffer {
    text: Vec<char>,
    cursor: usize,
    /// 输入历史（↑/↓ 召回）。
    history: Vec<String>,
    history_cursor: Option<usize>,
    draft: String,
}

impl EditBuffer {
    pub fn new() -> EditBuffer {
        EditBuffer::default()
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.text = text.chars().collect();
        self.cursor = self.text.len();
        self.history_cursor = None;
        self.draft.clear();
    }

    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.text.len());
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.history_cursor = None;
        self.draft.clear();
    }

    pub fn insert(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, s: &str) {
        for ch in s.chars() {
            self.insert(ch);
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.text.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        }
    }

    pub fn delete_word(&mut self) {
        while self.cursor > 0 && self.text[self.cursor - 1].is_whitespace() {
            self.backspace();
        }
        while self.cursor > 0 && !self.text[self.cursor - 1].is_whitespace() {
            self.backspace();
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.text.len();
    }

    pub fn push_history(&mut self, entry: impl Into<String>) {
        let entry = entry.into();
        if entry.trim().is_empty() {
            return;
        }
        if self
            .history
            .last()
            .map(|last| last == &entry)
            .unwrap_or(false)
        {
            return;
        }
        self.history.push(entry);
    }

    pub fn history(&self) -> &[String] {
        &self.history
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.history_cursor {
            None => {
                self.draft = self.text();
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.history_cursor = Some(next);
        let text = self.history[next].clone();
        self.set_text_no_reset(&text);
    }

    pub fn history_next(&mut self) {
        match self.history_cursor {
            None => {}
            Some(i) if i + 1 < self.history.len() => {
                self.history_cursor = Some(i + 1);
                let text = self.history[i + 1].clone();
                self.set_text_no_reset(&text);
            }
            Some(_) => {
                self.history_cursor = None;
                let draft = self.draft.clone();
                self.set_text_no_reset(&draft);
            }
        }
    }

    fn set_text_no_reset(&mut self, text: &str) {
        self.text = text.chars().collect();
        self.cursor = self.text.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_edit_roundtrip() {
        let mut buf = EditBuffer::new();
        buf.insert_str("example.com");
        assert_eq!(buf.text(), "example.com");
        buf.home();
        buf.insert('x');
        assert_eq!(buf.text(), "xexample.com");
        buf.end();
        buf.backspace();
        assert_eq!(buf.text(), "xexample.co");
        buf.delete();
        assert_eq!(buf.text(), "xexample.co");
    }

    #[test]
    fn word_delete_removes_last_word() {
        let mut buf = EditBuffer::new();
        buf.insert_str("rust http client");
        buf.delete_word();
        assert_eq!(buf.text(), "rust http ");
    }

    #[test]
    fn history_recall_and_draft_restore() {
        let mut buf = EditBuffer::new();
        buf.push_history("first");
        buf.push_history("second");
        buf.insert_str("draft");
        buf.history_prev();
        assert_eq!(buf.text(), "second");
        buf.history_prev();
        assert_eq!(buf.text(), "first");
        buf.history_prev();
        assert_eq!(buf.text(), "first");
        buf.history_next();
        assert_eq!(buf.text(), "second");
        buf.history_next();
        assert_eq!(buf.text(), "draft");
    }

    #[test]
    fn duplicate_history_entries_are_skipped() {
        let mut buf = EditBuffer::new();
        buf.push_history("same");
        buf.push_history("same");
        assert_eq!(buf.history().len(), 1);
        buf.push_history("  ");
        assert_eq!(buf.history().len(), 1);
    }
}

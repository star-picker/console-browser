//! cb-render: RenderDoc -> 按显示宽度折行的行/样式/链接索引。
//!
//! 只做布局，不碰终端：输出是纯数据结构，TUI 与测试都吃同一份结果。

use cb_extract::{Block, NoticeLevel, RenderDoc, Span, SpanStyle};
use std::collections::HashMap;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Normal,
    Bold,
    Italic,
    Code,
    Heading,
    Dim,
    Rule,
    NoticeInfo,
    NoticeWarn,
    NoticeError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Styled {
    pub text: String,
    pub style: Style,
    pub link: Option<u32>,
}

impl Styled {
    fn plain(text: impl Into<String>) -> Styled {
        Styled {
            text: text.into(),
            style: Style::Normal,
            link: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    pub cells: Vec<Styled>,
}

impl Line {
    pub fn text(&self) -> String {
        self.cells.iter().map(|c| c.text.as_str()).collect()
    }

    pub fn width(&self) -> usize {
        self.cells.iter().map(|c| c.text.width()).sum()
    }

    pub fn first_link(&self) -> Option<u32> {
        self.cells.iter().find_map(|c| c.link)
    }

    fn push(&mut self, cell: Styled) {
        self.cells.push(cell);
    }
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub width: u16,
    pub lines: Vec<Line>,
    /// 链接 id -> 首次出现的行号。
    pub link_lines: HashMap<u32, usize>,
    /// 查找用：每行的小写纯文本。
    lower_lines: Vec<String>,
}

fn span_style(style: &SpanStyle) -> Style {
    match style {
        SpanStyle::Normal => Style::Normal,
        SpanStyle::Bold => Style::Bold,
        SpanStyle::Italic => Style::Italic,
        SpanStyle::Code => Style::Code,
    }
}

fn to_styled(spans: &[Span]) -> Vec<Styled> {
    spans
        .iter()
        .map(|s| Styled {
            text: s.text.clone(),
            style: span_style(&s.style),
            link: s.link,
        })
        .collect()
}

fn char_width(ch: char) -> usize {
    ch.width().unwrap_or(0)
}

/// 把一段带样式的文本按显示宽度折行（优先在空白处断）。
#[allow(unused_assignments)]
fn wrap_cells(cells: &[Styled], width: usize, prefix: &str, indent: &str) -> Vec<Line> {
    let width = width.max(8);
    let mut lines: Vec<Line> = Vec::new();
    let mut cur = Line::default();
    let mut cur_width = 0usize;
    let prefix_width = prefix.width();
    let indent_width = indent.width();
    let mut first_line = true;
    let mut pending_space = false;

    let start_line = |line: &mut Line, first: bool| {
        if first {
            if !prefix.is_empty() {
                line.push(Styled::plain(prefix));
            }
        } else if !indent.is_empty() {
            line.push(Styled::plain(indent));
        }
    };
    start_line(&mut cur, true);
    cur_width = if first_line {
        prefix_width
    } else {
        indent_width
    };

    let budget = width
        .saturating_sub(if first_line {
            prefix_width
        } else {
            indent_width
        })
        .max(4);
    let mut used = 0usize;

    for cell in cells {
        let mut buf = String::new();
        for ch in cell.text.chars() {
            if ch == '\n' {
                if !buf.is_empty() {
                    cur.push(Styled {
                        text: std::mem::take(&mut buf),
                        style: cell.style,
                        link: cell.link,
                    });
                }
                lines.push(std::mem::take(&mut cur));
                first_line = false;
                start_line(&mut cur, false);
                cur_width = indent_width;
                used = 0;
                continue;
            }
            let w = char_width(ch);
            if used + w > budget {
                if pending_space || ch == ' ' {
                    // 在空白处断行：丢掉这个空格
                }
                if !buf.is_empty() {
                    let text = std::mem::take(&mut buf);
                    cur_width += text.width();
                    cur.push(Styled {
                        text,
                        style: cell.style,
                        link: cell.link,
                    });
                }
                lines.push(std::mem::take(&mut cur));
                first_line = false;
                start_line(&mut cur, false);
                cur_width = indent_width;
                used = 0;
                pending_space = false;
                if ch == ' ' {
                    continue;
                }
            }
            pending_space = ch == ' ';
            buf.push(ch);
            used += w;
        }
        if !buf.is_empty() {
            cur_width += buf.width();
            cur.push(Styled {
                text: buf,
                style: cell.style,
                link: cell.link,
            });
        }
    }
    let _ = cur_width;
    lines.push(cur);
    lines
}

fn text_block(text: &str, style: Style, width: usize) -> Vec<Line> {
    wrap_cells(
        &[Styled {
            text: text.to_string(),
            style,
            link: None,
        }],
        width,
        "",
        "",
    )
}

impl Layout {
    pub fn new(doc: &RenderDoc, width: u16) -> Layout {
        let width = width.max(8);
        let mut layout = Layout {
            width,
            lines: Vec::new(),
            link_lines: HashMap::new(),
            lower_lines: Vec::new(),
        };
        let mut blocks_out: Vec<Line> = Vec::new();
        for block in &doc.blocks {
            layout.render_block(block, width as usize, &mut blocks_out);
            blocks_out.push(Line::default());
        }
        for line in blocks_out {
            layout.push_line(line);
        }
        layout
    }

    fn push_line(&mut self, line: Line) {
        if let Some(link) = line.first_link() {
            self.link_lines.entry(link).or_insert(self.lines.len());
        }
        self.lower_lines.push(line.text().to_lowercase());
        self.lines.push(line);
    }

    fn render_block(&mut self, block: &Block, width: usize, out: &mut Vec<Line>) {
        match block {
            Block::Heading { level, spans } => {
                // 浏览器式：不画 "#"，只留层级间距；标题本身加粗靠 Style::Heading。
                if !out.is_empty() {
                    out.push(Line::default());
                }
                let _ = level;
                for line in wrap_cells(&to_styled(spans), width, "", "") {
                    out.push(line);
                }
            }
            Block::Paragraph { spans } => {
                out.extend(wrap_cells(&to_styled(spans), width, "", ""));
            }
            Block::ListItem {
                depth,
                marker,
                spans,
            } => {
                let prefix = format!("{}{}", "  ".repeat(*depth as usize), marker);
                let indent = format!(
                    "{}{}",
                    "  ".repeat(*depth as usize),
                    " ".repeat(marker.width().max(2))
                );
                out.extend(wrap_cells(&to_styled(spans), width, &prefix, &indent));
            }
            Block::Code { lang, lines } => {
                if !lang.is_empty() {
                    out.push(Line {
                        cells: vec![Styled {
                            text: format!("[{lang}]"),
                            style: Style::Dim,
                            link: None,
                        }],
                    });
                }
                for code_line in lines {
                    out.push(Line {
                        cells: vec![Styled {
                            text: code_line.clone(),
                            style: Style::Code,
                            link: None,
                        }],
                    });
                }
            }
            Block::Quote { blocks } => {
                let inner_width = width.saturating_sub(2).max(8);
                let mut inner: Vec<Line> = Vec::new();
                for child in blocks {
                    self.render_block(child, inner_width, &mut inner);
                }
                for line in inner {
                    let mut quoted = Line {
                        cells: vec![Styled {
                            text: "│ ".to_string(),
                            style: Style::Dim,
                            link: None,
                        }],
                    };
                    for cell in line.cells {
                        quoted.push(cell);
                    }
                    out.push(quoted);
                }
            }
            Block::Table { head, rows } => {
                let cols = head
                    .len()
                    .max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
                if cols == 0 {
                    return;
                }
                let share = (width.saturating_sub((cols - 1) * 3) / cols).max(4);
                let render_row = |cells: &[String], out: &mut Vec<Line>, style: Style| {
                    let mut wrapped: Vec<Vec<Line>> = Vec::new();
                    for i in 0..cols {
                        let text = cells.get(i).cloned().unwrap_or_default();
                        wrapped.push(wrap_cells(
                            &[Styled {
                                text,
                                style,
                                link: None,
                            }],
                            share,
                            "",
                            "",
                        ));
                    }
                    let height = wrapped.iter().map(|c| c.len()).max().unwrap_or(1);
                    for row_idx in 0..height {
                        let mut line = Line::default();
                        for (i, col) in wrapped.iter().enumerate() {
                            if i > 0 {
                                line.push(Styled {
                                    text: " │ ".to_string(),
                                    style: Style::Dim,
                                    link: None,
                                });
                            }
                            let text = col.get(row_idx).map(|l| l.text()).unwrap_or_default();
                            let pad = share.saturating_sub(text.width());
                            line.push(Styled {
                                text: format!("{text}{}", " ".repeat(pad)),
                                style,
                                link: None,
                            });
                        }
                        out.push(line);
                    }
                };
                render_row(head, out, Style::Bold);
                out.push(Line {
                    cells: vec![Styled {
                        text: "─".repeat(width.min(80)),
                        style: Style::Rule,
                        link: None,
                    }],
                });
                for row in rows {
                    render_row(row, out, Style::Normal);
                }
            }
            Block::Rule => {
                out.push(Line {
                    cells: vec![Styled {
                        text: "─".repeat(width.min(80)),
                        style: Style::Rule,
                        link: None,
                    }],
                });
            }
            Block::Image { alt, url } => {
                let label = if alt.is_empty() {
                    format!("[image] {url}")
                } else {
                    format!("[image: {alt}] {url}")
                };
                for line in text_block(&label, Style::Dim, width) {
                    out.push(line);
                }
            }
            Block::Notice {
                level,
                title,
                spans,
            } => {
                let marker = match level {
                    NoticeLevel::Info => "[i] ",
                    NoticeLevel::Warn => "[!] ",
                    NoticeLevel::Error => "[x] ",
                };
                let style = match level {
                    NoticeLevel::Info => Style::NoticeInfo,
                    NoticeLevel::Warn => Style::NoticeWarn,
                    NoticeLevel::Error => Style::NoticeError,
                };
                let heading = if title.is_empty() {
                    marker.to_string()
                } else {
                    format!("{marker}{title}")
                };
                out.push(Line {
                    cells: vec![Styled {
                        text: heading,
                        style,
                        link: None,
                    }],
                });
                out.extend(wrap_cells(&to_styled(spans), width, "    ", "    "));
            }
        }
    }

    /// 页内查找（大小写不敏感），返回命中的行号。
    pub fn find(&self, needle: &str) -> Vec<usize> {
        if needle.trim().is_empty() {
            return Vec::new();
        }
        let needle = needle.to_lowercase();
        self.lower_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.contains(&needle))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn total_lines(&self) -> usize {
        self.lines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cb_extract::blocks::NoticeLevel;
    use cb_extract::RenderDoc;

    fn doc(blocks: Vec<Block>) -> RenderDoc {
        RenderDoc {
            title: "t".to_string(),
            blocks,
            links: Vec::new(),
        }
    }

    fn para(text: &str) -> Block {
        Block::Paragraph {
            spans: vec![Span::normal(text)],
        }
    }

    #[test]
    fn long_paragraph_wraps_within_width() {
        let d = doc(vec![para(&"word ".repeat(80))]);
        let layout = Layout::new(&d, 40);
        assert!(layout.total_lines() > 1);
        for line in &layout.lines {
            assert!(
                line.width() <= 40,
                "line too wide: {} ({})",
                line.width(),
                line.text()
            );
        }
    }

    #[test]
    fn cjk_wraps_by_display_width() {
        let d = doc(vec![para(&"中文".repeat(40))]);
        let layout = Layout::new(&d, 21);
        for line in &layout.lines {
            assert!(line.width() <= 21, "CJK line too wide: {}", line.width());
        }
    }

    #[test]
    fn code_lines_are_not_wrapped() {
        let d = doc(vec![Block::Code {
            lang: "rust".to_string(),
            lines: vec!["let x = 1;".to_string(), "a".repeat(300)],
        }]);
        let layout = Layout::new(&d, 20);
        assert!(layout.lines.iter().any(|l| l.text().len() >= 300));
    }

    #[test]
    fn table_columns_align_and_head_is_bold() {
        let d = doc(vec![Block::Table {
            head: vec!["Key".to_string(), "Value".to_string()],
            rows: vec![vec!["a".to_string(), "1".to_string()]],
        }]);
        let layout = Layout::new(&d, 40);
        let head_line = layout
            .lines
            .iter()
            .find(|l| l.text().contains("Key"))
            .expect("head");
        assert!(head_line.cells.iter().any(|c| c.style == Style::Bold));
        assert!(layout.lines.iter().any(|l| l.text().starts_with('─')));
    }

    #[test]
    fn links_map_to_their_first_line() {
        let link = Span {
            text: "click".to_string(),
            style: SpanStyle::Normal,
            link: Some(7),
        };
        let d = RenderDoc {
            title: "t".to_string(),
            blocks: vec![Block::Paragraph {
                spans: vec![Span::normal("see "), link],
            }],
            links: Vec::new(),
        };
        let layout = Layout::new(&d, 40);
        assert_eq!(layout.link_lines.get(&7).copied(), Some(0));
    }

    #[test]
    fn find_is_case_insensitive_and_finds_all_hits() {
        let d = doc(vec![para("Rust here"), para("and rust there")]);
        let layout = Layout::new(&d, 40);
        assert_eq!(layout.find("rust").len(), 2);
        assert!(layout.find("").is_empty());
    }

    #[test]
    fn notice_blocks_get_markers() {
        let d = doc(vec![Block::Notice {
            level: NoticeLevel::Error,
            title: "Blocked".to_string(),
            spans: vec![Span::normal("try again")],
        }]);
        let layout = Layout::new(&d, 40);
        assert!(layout.lines[0].text().starts_with("[x] Blocked"));
    }

    #[test]
    fn quote_prefixes_every_line() {
        let d = doc(vec![Block::Quote {
            blocks: vec![para(
                "quoted text that will wrap because it is fairly long indeed",
            )],
        }]);
        let layout = Layout::new(&d, 24);
        for line in &layout.lines {
            if !line.text().trim().is_empty() {
                assert!(
                    line.text().starts_with('│'),
                    "missing quote prefix: {}",
                    line.text()
                );
            }
        }
    }
}

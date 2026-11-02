//! 结构化文档模型：给 TUI / webui 渲染用。
//!
//! 与 markdown 序列化并存（markdown 是给 AI 的，Block 是给屏幕的）。
//! 裁决：TUI 走这条独立 walker，不动现有 markdown 链路，保证 AI 那侧的 golden 零风险；
//! 代价是一次解析跑两遍（毫秒级）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanStyle {
    Normal,
    Bold,
    Italic,
    Code,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    pub style: SpanStyle,
    /// 指向 RenderDoc.links 的下标（不是 URL 本体）。
    pub link: Option<u32>,
}

impl Span {
    pub fn normal(text: impl Into<String>) -> Span {
        Span {
            text: text.into(),
            style: SpanStyle::Normal,
            link: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoticeLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Block {
    Heading {
        level: u8,
        spans: Vec<Span>,
    },
    Paragraph {
        spans: Vec<Span>,
    },
    ListItem {
        depth: u8,
        marker: String,
        spans: Vec<Span>,
    },
    Code {
        lang: String,
        lines: Vec<String>,
    },
    Quote {
        blocks: Vec<Block>,
    },
    Table {
        head: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Rule,
    Image {
        alt: String,
        url: String,
    },
    /// 错误页 / 需登录页 / 空状态：TUI 直接画成一块通知。
    Notice {
        level: NoticeLevel,
        title: String,
        spans: Vec<Span>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRef {
    pub id: u32,
    pub url: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderDoc {
    pub title: String,
    pub blocks: Vec<Block>,
    pub links: Vec<LinkRef>,
}

impl RenderDoc {
    pub fn link(&self, id: u32) -> Option<&LinkRef> {
        self.links.get(id as usize)
    }

    /// 纯文本长度估算，给「页面是否几乎为空」这类判断用。
    pub fn text_len(&self) -> usize {
        self.blocks.iter().map(block_len).sum()
    }
}

fn span_len(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.text.chars().count()).sum()
}

fn block_len(block: &Block) -> usize {
    match block {
        Block::Heading { spans, .. } => span_len(spans),
        Block::Paragraph { spans } => span_len(spans),
        Block::ListItem { spans, .. } => span_len(spans),
        Block::Code { lines, .. } => lines.iter().map(|l| l.chars().count()).sum(),
        Block::Quote { blocks } => blocks.iter().map(block_len).sum(),
        Block::Table { head, rows } => {
            head.iter().map(|c| c.chars().count()).sum::<usize>()
                + rows
                    .iter()
                    .flat_map(|r| r.iter())
                    .map(|c| c.chars().count())
                    .sum::<usize>()
        }
        Block::Rule => 0,
        Block::Image { alt, .. } => alt.chars().count(),
        Block::Notice { title, spans, .. } => title.chars().count() + span_len(spans),
    }
}

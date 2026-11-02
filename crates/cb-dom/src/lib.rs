//! cb-dom: HTML parse + CSS select + main-content detection (readability-ish scoring).

use scraper::{ElementRef, Html, Selector};

/// 结构上永远不是正文的标签。
pub const SKIP_TAGS: &[&str] = &[
    "script", "style", "noscript", "nav", "aside", "form", "svg", "iframe", "template", "button",
    "select", "input", "link", "meta", "canvas", "video", "audio",
];

/// class/id 词元命中这些就丢掉。按「词元」匹配，不做子串匹配
/// （子串会把 download-/head-/shareholder 误杀）。
pub const SKIP_TOKENS: &[&str] = &[
    "sidebar",
    "advert",
    "ads",
    "adsense",
    "adslot",
    "banner",
    "cookie",
    "menu",
    "navbar",
    "nav",
    "share",
    "social",
    "comment",
    "comments",
    "promo",
    "sponsor",
    "subscribe",
    "newsletter",
    "trending",
    "paywall",
    "related",
];

/// role 命中这些一律不是内容。
pub const SKIP_ROLES: &[&str] = &[
    "navigation",
    "banner",
    "contentinfo",
    "complementary",
    "search",
];

fn tokens_of(element: &ElementRef<'_>) -> Vec<String> {
    ["class", "id"]
        .iter()
        .flat_map(|attr| {
            element
                .value()
                .attr(attr)
                .unwrap_or_default()
                .split_whitespace()
                .map(|t| t.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .collect()
}

fn token_is_noise(token: &str) -> bool {
    if SKIP_TOKENS.contains(&token) {
        return true;
    }
    // ad / ad-xxx / advert-xxx
    token == "ad" || token.starts_with("ad-") || token.starts_with("advert")
}

pub fn is_noise(element: &ElementRef<'_>) -> bool {
    let name = element.value().name();
    if SKIP_TAGS.contains(&name) {
        return true;
    }
    if let Some(role) = element.value().attr("role") {
        if SKIP_ROLES.contains(&role.to_ascii_lowercase().as_str()) {
            return true;
        }
    }
    tokens_of(element).iter().any(|t| token_is_noise(t))
}

/// 页面级页眉/页脚：只有在正文容器内部时才保留（文章的 byline、标题常在 header 里）。
pub fn is_page_chrome(element: &ElementRef<'_>) -> bool {
    let name = element.value().name();
    if name == "header" || name == "footer" {
        return true;
    }
    matches!(
        element.value().attr("role").map(|r| r.to_ascii_lowercase()),
        Some(role) if role == "banner" || role == "contentinfo"
    )
}
pub struct Document {
    html: Html,
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

impl Document {
    pub fn parse(html: &str) -> Document {
        Document {
            html: Html::parse_document(html),
        }
    }

    pub fn title(&self) -> Option<String> {
        self.html
            .select(&sel("title"))
            .next()
            .map(|t| t.text().collect::<String>().trim().to_string())
            .filter(|t| !t.is_empty())
    }

    pub fn select(&self, css: &str) -> Vec<ElementRef<'_>> {
        match Selector::parse(css) {
            Ok(s) => self.html.select(&s).collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn text_of(&self, element: ElementRef<'_>) -> String {
        element.text().collect::<Vec<_>>().join(" ")
    }

    pub fn link_density(&self, element: ElementRef<'_>) -> f64 {
        let total = self.text_of(element).chars().count();
        if total == 0 {
            return 0.0;
        }
        let linked: usize = element
            .select(&sel("a"))
            .map(|a| a.text().collect::<String>().chars().count())
            .sum();
        (linked as f64 / total as f64).clamp(0.0, 1.0)
    }

    /// 选正文容器。
    ///
    /// 两段式：先看语义容器（article / main / role=main）；没有才退回通用容器。
    /// 通用容器要排除"整页包裹层"（文本接近 body 的那种 div），否则外层总赢。
    pub fn main_content(&self) -> Option<ElementRef<'_>> {
        if let Some(el) = self.best_container("article, main, [role=main]", 40) {
            return Some(el);
        }
        let body_len = self
            .body()
            .map(|b| self.text_of(b).chars().count())
            .unwrap_or(0);
        if let Some(el) = self.best_container("div, section, td", 80) {
            let len = self.text_of(el).chars().count();
            // 占了整页八成以上文字，说明它只是包裹层，不是正文。
            if body_len == 0 || (len as f64) <= body_len as f64 * 0.85 {
                return Some(el);
            }
        }
        self.body()
    }

    fn best_container(&self, css: &str, min_len: usize) -> Option<ElementRef<'_>> {
        let mut best: Option<(f64, ElementRef<'_>)> = None;
        for el in self.select(css) {
            if is_noise(&el) {
                continue;
            }
            let len = self.text_of(el).chars().count();
            if len < min_len {
                continue;
            }
            let density = self.link_density(el);
            // 链接堆（目录、文件列表、导航）不是正文。
            if density > 0.4 {
                continue;
            }
            let bonus = match el.value().name() {
                "article" => 300.0,
                "main" => 250.0,
                _ => match el.value().attr("role") {
                    Some("main") => 200.0,
                    _ => 0.0,
                },
            };
            // 链接密度平方惩罚，压住"半正文半导航"的容器。
            let score = len as f64 * (1.0 - density).powi(2) + bonus;
            // 分数接近时优先更深的容器（文档序靠后 = 更具体）。
            if best.as_ref().is_none_or(|(b, _)| score >= *b) {
                best = Some((score, el));
            }
        }
        best.map(|(_, el)| el)
    }

    pub fn body(&self) -> Option<ElementRef<'_>> {
        self.html.select(&sel("body")).next()
    }
}

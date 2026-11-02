//! cb-dom: HTML parse + CSS select + main-content detection (readability-ish scoring).

use scraper::{ElementRef, Html, Selector};

pub const SKIP_TAGS: &[&str] = &[
    "script", "style", "noscript", "nav", "footer", "aside", "header", "form", "svg", "iframe",
    "template", "button", "select", "input", "link", "meta", "canvas", "video", "audio",
];

pub const SKIP_CLASS_HINTS: &[&str] = &[
    "sidebar",
    "sidebar-ad",
    "ad-",
    "-ad",
    "ads",
    "advert",
    "banner",
    "cookie",
    "menu",
    "navbar",
    "nav-",
    "share",
    "social",
    "comment",
    "promo",
    "sponsor",
    "subscribe",
    "trending",
];

pub fn is_noise(element: &ElementRef<'_>) -> bool {
    let name = element.value().name();
    if SKIP_TAGS.contains(&name) {
        return true;
    }
    let attr = [
        element.value().attr("class").unwrap_or_default(),
        element.value().attr("id").unwrap_or_default(),
        element.value().attr("role").unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_lowercase();
    if attr.is_empty() {
        return false;
    }
    SKIP_CLASS_HINTS.iter().any(|hint| attr.contains(hint))
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

    /// 选正文容器：文本长度 ×（1 − 链接密度）+ 语义标签加成。
    pub fn main_content(&self) -> Option<ElementRef<'_>> {
        let candidates = self.select("article, main, [role=main], div, section, td");
        let mut best: Option<(f64, ElementRef<'_>)> = None;
        for el in candidates {
            if is_noise(&el) {
                continue;
            }
            let len = self.text_of(el).chars().count();
            if len < 80 {
                continue;
            }
            let bonus = match el.value().name() {
                "article" | "main" => 200.0,
                _ => match el.value().attr("role") {
                    Some("main") => 150.0,
                    _ => 0.0,
                },
            };
            let score = len as f64 * (1.0 - self.link_density(el)) + bonus;
            if best.as_ref().map_or(true, |(b, _)| score > *b) {
                best = Some((score, el));
            }
        }
        best.map(|(_, el)| el)
            .or_else(|| self.html.select(&sel("body")).next())
    }

    pub fn body(&self) -> Option<ElementRef<'_>> {
        self.html.select(&sel("body")).next()
    }
}

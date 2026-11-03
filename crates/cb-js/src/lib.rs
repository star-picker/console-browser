//! cb-js: 最小 JS 运行时（QuickJS）+ DOM/BOM 桥。
//!
//! 目标不是实现浏览器，而是让「靠 JS 渲染内容」的页面能被读出正文：
//! - 执行 inline / 外部脚本；
//! - 提供 document.title / document.body.innerText / document.querySelectorAll 的
//!   最小实现（后续接真实 DOM）；
//! - fetch 走 cb-net（保留 TLS 指纹与 cookie），不直连网络。

use anyhow::Result;

/// 元素快照：够 querySelectorAll 做 id/class/tag 三种匹配。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ElSnap {
    pub tag: String,
    pub id: String,
    pub class: String,
    pub text: String,
}

/// 一个 JS 运行时；一个上下文执行一个页面的脚本。
pub struct JsRuntime {
    ctx: rquickjs::Context,
    _rt: rquickjs::Runtime,
}

impl JsRuntime {
    pub fn new() -> Result<JsRuntime> {
        let rt = rquickjs::Runtime::new()?;
        let ctx = rquickjs::Context::full(&rt)?;
        Ok(JsRuntime { ctx, _rt: rt })
    }

    /// 装一个最小 DOM/BOM：document.title / document.body.innerText 可读，
    /// scripts 里常见的取值就能跑通（真正的 DOM 变更留给后续迭代）。
    pub fn install_document(&self, title: &str, body_text: &str) -> Result<()> {
        let prelude = format!(
            "globalThis.document = {{ title: {}, body: {{ innerText: {} }} }};",
            json_literal(title),
            json_literal(body_text)
        );
        self.ctx.with(|ctx| ctx.eval::<(), _>(prelude.as_bytes()))?;
        Ok(())
    }

    /// 从 HTML 装 DOM 桩：title / body.innerText / querySelectorAll（id|class|tag）。
    pub fn install_html(&self, html: &str) -> Result<()> {
        use scraper::{Html, Selector};
        let doc = Html::parse_document(html);
        let title = Selector::parse("title").ok();
        let body = Selector::parse("body").ok();
        let title_text = title
            .and_then(|sel| doc.select(&sel).next())
            .map(|t| t.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let body_text = body
            .and_then(|sel| doc.select(&sel).next())
            .map(|b| b.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let els = serde_json::to_string(&Self::elements(html)).unwrap_or_else(|_| "[]".to_string());
        let prelude = include_str!("prelude.js")
            .replace("__TITLE__", &json_literal(&title_text))
            .replace("__TEXT__", &json_literal(&body_text))
            .replace("__ELS__", &els)
            .replace("__URL__", "''")
            .replace("__LANG__", "''");
        self.ctx.with(|ctx| ctx.eval::<(), _>(prelude.as_bytes()))?;
        Ok(())
    }

    /// 装一个「预取缓存版 fetch」：调用方先把 URL 抓成 (url, body)，
    /// 脚本里的 fetch(url) 同步取缓存；没有缓存的 URL 返回 404 形状的响应。
    /// 这不是完整 fetch（无网络、无 Promise 调度），但足以让「先取数据再填 DOM」的脚本跑通。
    pub fn install_fetch(&self, entries: &[(String, String)]) -> Result<()> {
        let map = entries
            .iter()
            .map(|(url, body)| (url.clone(), body.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        let json = serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string());
        let js = format!(
            "globalThis.__cb_fetch_cache = {json};\n\
             globalThis.fetch = (url) => {{\n\
               const key = String(url);\n\
               const body = __cb_fetch_cache[key] ?? null;\n\
               const res = {{ ok: body !== null, status: body !== null ? 200 : 404,\n\
                 text: () => (body ?? ''),\n\
                 json: () => {{ try {{ return JSON.parse(body); }} catch (e) {{ return null; }} }},\n\
                 then: (cb) => {{ if (cb) cb(res); return res; }} }};\n\
               return res;\n\
             }};"
        );
        self.ctx.with(|ctx| ctx.eval::<(), _>(js.as_bytes()))?;
        Ok(())
    }
    /// 装 BOM 桩：location / localStorage / navigator / console。
    pub fn install_bom(&self, url: &str, lang: &str) -> Result<()> {
        let js = format!(
            "globalThis.location = {{ href: {}, host: {} }};",
            json_literal(url),
            json_literal(
                url.trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .split('/')
                    .next()
                    .unwrap_or("")
            )
        );
        let js = format!(
            "{js} globalThis.navigator = {{ userAgent: 'cb-js/1', language: '{}' }};",
            lang
        );
        let ls_js = concat!(
            "globalThis.localStorage = (function(){ const m = new Map(); return { ",
            "getItem: (k) => (m.has(k) ? m.get(k) : null), ",
            "setItem: (k, v) => { m.set(String(k), String(v)); }, ",
            "removeItem: (k) => { m.delete(k); }, clear: () => { m.clear(); } }; })();",
            "globalThis.__cb_log = []; globalThis.console = { log: (...a) => { __cb_log.push(a.map(String).join(' ')); } };"
        );
        let js = format!("{js}{ls_js}");
        self.ctx.with(|ctx| ctx.eval::<(), _>(js.as_bytes()))?;
        Ok(())
    }

    /// 元素快照（最多 400 个）：tag / id / class / text。
    pub fn elements(html: &str) -> Vec<ElSnap> {
        use scraper::{Html, Selector};
        let doc = Html::parse_document(html);
        let Ok(selector) = Selector::parse("*") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for el in doc.select(&selector) {
            out.push(ElSnap {
                tag: el.value().name().to_string(),
                id: el.value().attr("id").unwrap_or("").to_string(),
                class: el.value().attr("class").unwrap_or("").to_string(),
                text: el.text().collect::<Vec<_>>().join(" ").trim().to_string(),
            });
            if out.len() >= 400 {
                break;
            }
        }
        out
    }
    /// 执行一段脚本，返回最后表达式的字符串（对象则 JSON 序列化）。
    pub fn eval(&self, code: &str) -> Result<String> {
        let out = self.ctx.with(|ctx| {
            let value: rquickjs::Value = ctx.eval(code)?;
            if value.is_undefined() || value.is_null() {
                return Ok::<String, rquickjs::Error>(String::new());
            }
            if let Some(s) = value.as_string() {
                return Ok(s.to_string().unwrap_or_default());
            }
            let json = match ctx.json_stringify(value) {
                Ok(Some(s)) => s.to_string().unwrap_or_default(),
                _ => String::new(),
            };
            Ok(json)
        })?;
        Ok(out)
    }
}

fn json_literal(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_plain_expressions() {
        let rt = JsRuntime::new().expect("runtime");
        assert_eq!(rt.eval("1 + 1").expect("eval"), "2");
        assert_eq!(rt.eval("'cb'.toUpperCase()").expect("eval"), "CB");
    }

    #[test]
    fn prefetched_fetch_serves_cached_bodies() {
        let rt = JsRuntime::new().expect("runtime");
        rt.install_fetch(&[(
            "https://api.example/data".to_string(),
            "{\"title\":\"hi\"}".to_string(),
        )])
        .expect("install");
        assert_eq!(
            rt.eval("fetch('https://api.example/data').json().title")
                .expect("json"),
            "hi"
        );
        assert_eq!(
            rt.eval("fetch('https://api.example/missing').status")
                .expect("miss"),
            "404"
        );
    }

    #[test]
    fn bom_stubs_exist() {
        let rt = JsRuntime::new().expect("runtime");
        rt.install_bom("https://example.com/x?q=1", "zh-CN")
            .expect("bom");
        assert_eq!(rt.eval("location.host").expect("host"), "example.com");
        assert_eq!(rt.eval("navigator.language").expect("lang"), "zh-CN");
        assert_eq!(
            rt.eval("localStorage.setItem('k','v'); localStorage.getItem('k')")
                .expect("ls"),
            "v"
        );
    }

    #[test]
    fn install_html_exposes_queryselectorall() {
        let rt = JsRuntime::new().expect("runtime");
        let html = "<html><head><title>T</title></head><body><div class=\"a b\">一</div><div class=\"a\">二</div><p id=\"main\">三</p></body></html>";
        rt.install_html(html).expect("install");
        assert_eq!(rt.eval("document.title").expect("title"), "T");
        assert_eq!(
            rt.eval("document.querySelectorAll('.a').length")
                .expect("class"),
            "2"
        );
        assert_eq!(
            rt.eval("document.querySelectorAll('#main').length")
                .expect("id"),
            "1"
        );
        assert_eq!(
            rt.eval("document.querySelectorAll('p')[0].textContent")
                .expect("tag"),
            "三"
        );
    }

    #[test]
    fn document_stub_exposes_title_and_body_text() {
        let rt = JsRuntime::new().expect("runtime");
        rt.install_document("示例标题", "第一段").expect("install");
        assert_eq!(rt.eval("document.title").expect("eval"), "示例标题");
        assert_eq!(rt.eval("document.body.innerText").expect("eval"), "第一段");
        assert_eq!(rt.eval("document.title.length").expect("eval"), "4");
    }

    #[test]
    fn json_roundtrip_through_quickjs() {
        let rt = JsRuntime::new().expect("runtime");
        let out = rt
            .eval("JSON.stringify({title: 'hi', n: 2})")
            .expect("eval");
        assert!(out.contains("\"title\":\"hi\""), "{out}");
    }
}

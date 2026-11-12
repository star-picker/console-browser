//! cb-js: 最小 JS 运行时（QuickJS）+ DOM/BOM 桥。
//!
//! 目标不是实现浏览器，而是让「靠 JS 渲染内容」的页面能被读出正文：
//! - 执行 inline / 外部脚本；
//! - 提供 document.title / document.body.innerText / document.querySelectorAll 的
//!   最小实现（后续接真实 DOM）；
//! - fetch 走 cb-net（保留 TLS 指纹与 cookie），不直连网络。

use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// DOM 树节点：给 prelude 物化成带父子关系的元素对象。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DomNode {
    pub tag: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub attrs: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DomNode>,
}

/// 一个 JS 运行时；一个上下文执行一个页面的脚本。
pub struct JsRuntime {
    ctx: rquickjs::Context,
    /// 保持运行时存活；QuickJS 的 context 依赖它。
    #[allow(dead_code)]
    rt: rquickjs::Runtime,
    interrupted: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
}

impl JsRuntime {
    pub fn new() -> Result<JsRuntime> {
        let rt = rquickjs::Runtime::new()?;
        let ctx = rquickjs::Context::full(&rt)?;
        let interrupted = Arc::new(AtomicBool::new(false));
        let generation = Arc::new(AtomicU64::new(0));
        let flag = interrupted.clone();
        rt.set_interrupt_handler(Some(Box::new(move || flag.load(Ordering::Relaxed))));
        Ok(JsRuntime {
            ctx,
            rt,
            interrupted,
            generation,
        })
    }

    pub fn eval_with_timeout(&self, code: &str, timeout_ms: u64) -> Result<String> {
        self.interrupted.store(false, Ordering::SeqCst);
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let flag = self.interrupted.clone();
        let gen_arc = self.generation.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(timeout_ms));
            if gen_arc.load(Ordering::SeqCst) == gen {
                flag.store(true, Ordering::SeqCst);
            }
        });
        let out = self.eval(code);
        // 让超时线程失效：否则它稍后置位会误伤下一次 eval。
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.interrupted.store(false, Ordering::SeqCst);
        out
    }

    /// 驱动事件循环：先清微任务，再按到期时间跑定时器，直到没有待办或超时。
    /// 返回执行过的定时器回调次数。
    pub fn run_event_loop(&self, timeout_ms: u64) -> Result<usize> {
        self.interrupted.store(false, Ordering::SeqCst);
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let flag = self.interrupted.clone();
        let gen_arc = self.generation.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(timeout_ms));
            if gen_arc.load(Ordering::SeqCst) == gen {
                flag.store(true, Ordering::SeqCst);
            }
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let mut fired = 0usize;
        let outcome = loop {
            while self.ctx.with(|ctx| ctx.execute_pending_job()) {}
            let due: i64 = match self.ctx.with(|ctx| ctx.eval("__cb_fire_due()")) {
                Ok(value) => value,
                Err(err) => break Err(anyhow::anyhow!("{err}")),
            };
            if due > 0 {
                fired += due as usize;
                if fired > 100_000 {
                    break Err(anyhow::anyhow!("timer loop exceeded 100000 callbacks"));
                }
                continue;
            }
            let wait: i64 = match self.ctx.with(|ctx| ctx.eval("__cb_next_wait()")) {
                Ok(value) => value,
                Err(err) => break Err(anyhow::anyhow!("{err}")),
            };
            if wait < 0 {
                break Ok(fired);
            }
            let now = std::time::Instant::now();
            if now >= deadline {
                break Ok(fired);
            }
            let remaining = deadline.duration_since(now).as_millis() as u64;
            let nap = (wait as u64).min(remaining).max(1);
            std::thread::sleep(std::time::Duration::from_millis(nap));
        };
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.interrupted.store(false, Ordering::SeqCst);
        outcome
    }

    /// 页面里的外链脚本地址（相对路径按 base 解析），跳过 data:/javascript:。
    pub fn external_scripts(html: &str, base: &str) -> Vec<String> {
        use scraper::{Html, Selector};
        let doc = Html::parse_document(html);
        let Ok(selector) = Selector::parse("script[src]") else {
            return Vec::new();
        };
        let base_url = url::Url::parse(base).ok();
        let mut out = Vec::new();
        for el in doc.select(&selector) {
            let Some(src) = el.value().attr("src") else {
                continue;
            };
            let src = src.trim();
            if src.is_empty() || src.starts_with("data:") || src.starts_with("javascript:") {
                continue;
            }
            let absolute = match base_url.as_ref().and_then(|b| b.join(src).ok()) {
                Some(u) => u.to_string(),
                None => continue,
            };
            out.push(absolute);
            if out.len() >= 20 {
                break;
            }
        }
        out
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
        let tree = Self::document_tree(html);
        let tree_json =
            serde_json::to_string(&tree).unwrap_or_else(|_| "{\"tag\":\"html\"}".to_string());
        let prelude = include_str!("prelude.js")
            .replace("__TITLE__", &json_literal(&title_text))
            .replace("__TEXT__", &json_literal(&body_text))
            .replace("__TREE__", &tree_json)
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
    /// 页面里的内联脚本（跳过 src 外链、JSON-LD、模板），按文档顺序返回。
    pub fn inline_scripts(html: &str) -> Vec<String> {
        use scraper::{Html, Selector};
        let doc = Html::parse_document(html);
        let Ok(selector) = Selector::parse("script:not([src])") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for el in doc.select(&selector) {
            let kind = el.value().attr("type").unwrap_or("").to_ascii_lowercase();
            if kind.contains("json") || kind.contains("template") {
                continue;
            }
            let code = el.text().collect::<Vec<_>>().join("\n");
            let code = code.trim();
            if code.is_empty() {
                continue;
            }
            out.push(code.to_string());
            if out.len() >= 50 {
                break;
            }
        }
        out
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

    /// 把 HTML 解析成真实父子树（属性 + 直接文本 + 子元素），最多 8000 个元素。
    pub fn document_tree(html: &str) -> DomNode {
        use scraper::{Html, Selector};
        let doc = Html::parse_document(html);
        let mut budget = 0usize;
        if let Ok(selector) = Selector::parse("html") {
            if let Some(root) = doc.select(&selector).next() {
                return build_dom_node(root, &mut budget);
            }
        }
        DomNode {
            tag: "html".to_string(),
            attrs: Default::default(),
            text: String::new(),
            children: Vec::new(),
        }
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

const MAX_DOM_NODES: usize = 8_000;

fn collapse_dom_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn build_dom_node(el: scraper::ElementRef<'_>, budget: &mut usize) -> DomNode {
    *budget += 1;
    let mut attrs = std::collections::BTreeMap::new();
    for (name, value) in el.value().attrs() {
        attrs.insert(name.to_string(), value.to_string());
    }
    let mut text = String::new();
    let mut children = Vec::new();
    if *budget < MAX_DOM_NODES {
        for child in el.children() {
            match child.value() {
                scraper::Node::Text(node) => text.push_str(&node.text),
                scraper::Node::Element(_) => {
                    if *budget >= MAX_DOM_NODES {
                        break;
                    }
                    if let Some(child_el) = scraper::ElementRef::wrap(child) {
                        children.push(build_dom_node(child_el, budget));
                    }
                }
                _ => {}
            }
        }
    }
    DomNode {
        tag: el.value().name().to_string(),
        attrs,
        text: collapse_dom_text(&text),
        children,
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
    fn external_scripts_resolve_relative_urls() {
        let html = r#"<html><body><script src="/a.js"></script><script src="https://cdn.example/b.js"></script><script src="data:text/javascript,1"></script></body></html>"#;
        let urls = JsRuntime::external_scripts(html, "https://site.example/page");
        assert_eq!(
            urls,
            vec![
                "https://site.example/a.js".to_string(),
                "https://cdn.example/b.js".to_string()
            ]
        );
    }

    #[test]
    fn inline_scripts_are_extracted_in_order() {
        let html = r#"<html><body><script type="application/json">{"a":1}</script><script>var a = 1;</script><script src="x.js"></script><script>var b = 2;</script></body></html>"#;
        let scripts = JsRuntime::inline_scripts(html);
        assert_eq!(scripts.len(), 2, "{scripts:?}");
        assert!(scripts[0].contains("var a = 1"));
        assert!(scripts[1].contains("var b = 2"));
    }

    #[test]
    fn infinite_loop_is_interrupted() {
        let rt = JsRuntime::new().expect("runtime");
        let started = std::time::Instant::now();
        assert!(rt.eval_with_timeout("while (true) {}", 60).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        assert_eq!(rt.eval("1 + 2").expect("alive"), "3");
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

    fn js() -> JsRuntime {
        let rt = JsRuntime::new().expect("runtime");
        rt.install_html("<html><head><title>T</title></head><body><div class=\"a b\">一</div><div class=\"a\">二</div><p id=\"main\">三</p></body></html>")
            .expect("install html");
        rt
    }

    #[test]
    fn event_loop_runs_timeouts_in_order() {
        let rt = js();
        rt.eval(
            "__cb_log = []; setTimeout(function(){ __cb_log.push('a'); }, 0); setTimeout(function(){ __cb_log.push('b'); }, 0);",
        )
        .expect("schedule");
        let fired = rt.run_event_loop(500).expect("loop");
        assert_eq!(fired, 2, "both timers must fire");
        assert_eq!(rt.eval("__cb_log.join(',')").expect("log"), "a,b");
    }

    #[test]
    fn event_loop_runs_nested_and_microtasks() {
        let rt = js();
        rt.eval(
            "__cb_log = []; setTimeout(function(){ __cb_log.push('a'); setTimeout(function(){ __cb_log.push('b'); }, 0); }, 0); Promise.resolve().then(function(){ __cb_log.push('micro'); });",
        )
        .expect("schedule");
        rt.run_event_loop(500).expect("loop");
        let log = rt.eval("__cb_log.join(',')").expect("log");
        assert_eq!(log, "micro,a,b", "microtasks run before timers");
    }

    #[test]
    fn clear_timeout_cancels_the_callback() {
        let rt = js();
        rt.eval(
            "__cb_log = []; const id = setTimeout(function(){ __cb_log.push('x'); }, 0); clearTimeout(id);",
        )
        .expect("schedule");
        let fired = rt.run_event_loop(100).expect("loop");
        assert_eq!(fired, 0);
        assert_eq!(rt.eval("__cb_log.length").expect("log"), "0");
    }

    #[test]
    fn interval_is_capped_by_the_loop_budget() {
        let rt = js();
        rt.eval(
            "globalThis.__cb_count = 0; setInterval(function(){ globalThis.__cb_count++; }, 10);",
        )
        .expect("schedule");
        let started = std::time::Instant::now();
        let fired = rt.run_event_loop(60).expect("loop");
        assert!(fired >= 1, "interval must fire at least once");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "loop must respect the budget"
        );
        assert!(
            rt.eval("__cb_count")
                .expect("count")
                .parse::<i64>()
                .unwrap_or(0)
                >= 1
        );
    }

    #[test]
    fn dom_attributes_text_and_events_work_locally() {
        let rt = js();
        let out = rt
            .eval(
                "const el = document.querySelector('.a'); el.setAttribute('data-x', '1'); el.textContent = 'changed'; let clicks = 0; el.addEventListener('click', function(){ clicks++; }); el.click(); el.click(); [el.getAttribute('data-x'), el.textContent, clicks].join('|')",
            )
            .expect("dom ops");
        assert_eq!(out, "1|changed|2");
        assert_eq!(
            rt.eval("document.querySelectorAll('.a').length")
                .expect("len"),
            "2"
        );
        assert_eq!(
            rt.eval("document.querySelector('#main').textContent")
                .expect("q"),
            "三"
        );
    }

    #[test]
    fn create_element_and_append_child_are_queryable() {
        let rt = js();
        let out = rt
            .eval(
                "const span = document.createElement('span'); span.id = 'new'; span.textContent = 'x'; document.body.appendChild(span); document.getElementById('new').textContent",
            )
            .expect("append");
        assert_eq!(out, "x");
        assert_eq!(
            rt.eval("document.querySelectorAll('span').length")
                .expect("count"),
            "1"
        );
    }
    fn tree_js() -> JsRuntime {
        let rt = JsRuntime::new().expect("runtime");
        let html = r#"<html><head><title>Doc</title></head><body><div id="app" data-kind="root"><section class="x"><p>a</p><p class="y" data-k="v">b</p></section><p>c</p></div></body></html>"#;
        rt.install_html(html).expect("install tree");
        rt
    }

    #[test]
    fn selectors_support_descendants_children_attributes_and_nth_child() {
        let rt = tree_js();
        let count = |sel: &str| {
            rt.eval(&format!("document.querySelectorAll('{sel}').length"))
                .expect("count")
        };
        assert_eq!(count("#app p"), "3", "descendant combinator");
        assert_eq!(count("#app > p"), "1", "child combinator");
        assert_eq!(count("section p"), "2");
        assert_eq!(count("p.y"), "1");
        assert_eq!(count("[data-k=v]"), "1", "attribute selector");
        assert_eq!(count("p:first-child"), "1");
        assert_eq!(count("p:nth-child(2)"), "2");
        assert_eq!(count("section p, #app > p"), "3", "selector list");
        assert_eq!(
            rt.eval("document.querySelector('p.y').textContent")
                .expect("text"),
            "b"
        );
    }

    #[test]
    fn tree_links_and_recursive_text_content() {
        let rt = tree_js();
        let out = rt
            .eval(
                "const section = document.querySelector('section'); [section.parentElement.id, document.body.parentElement.tag, document.getElementById('app').children.length, section.textContent, section.innerText].join('|')",
            )
            .expect("tree");
        assert_eq!(out, "app|html|2|ab|a b");
    }

    #[test]
    fn append_remove_and_insert_before_update_the_tree() {
        let rt = tree_js();
        let out = rt
            .eval(
                "const app = document.getElementById('app'); const p = document.createElement('p'); p.id = 'new'; p.textContent = 'n'; app.appendChild(p); const afterAdd = document.querySelectorAll('#app > p').length; app.removeChild(p); const afterRemove = document.querySelectorAll('#app > p').length; const hr = document.createElement('hr'); const first = app.firstElementChild; app.insertBefore(hr, first); [afterAdd, afterRemove, app.firstElementChild.tag, document.getElementById('new') === null].join('|')",
            )
            .expect("mutate");
        assert_eq!(out, "2|1|hr|true");
    }

    #[test]
    fn class_list_and_attribute_helpers_work() {
        let rt = tree_js();
        let out = rt
            .eval(
                "const el = document.querySelector('.x'); el.classList.add('z'); const hasZ = el.classList.contains('z'); const toggled = el.classList.toggle('x'); el.setAttribute('data-a', '1'); const has = el.hasAttribute('data-a'); el.removeAttribute('data-a'); [hasZ, toggled, el.className, has, el.hasAttribute('data-a')].join('|')",
            )
            .expect("classlist");
        assert_eq!(out, "true|false|z|true|false");
    }

    #[test]
    fn detached_elements_are_not_in_document_queries_until_appended() {
        let rt = tree_js();
        let out = rt
            .eval(
                "const tmp = document.createElement('div'); tmp.id = 'tmp'; const before = document.querySelectorAll('div').length; document.body.appendChild(tmp); const after = document.querySelectorAll('div').length; [before, after, document.getElementById('tmp') !== null].join('|')",
            )
            .expect("detached");
        assert_eq!(out, "1|2|true");
    }

    #[test]
    fn title_setter_updates_the_title_element() {
        let rt = tree_js();
        let out = rt
            .eval("document.title = 'New'; document.title + '|' + document.querySelector('title').textContent")
            .expect("title");
        assert_eq!(out, "New|New");
    }
}

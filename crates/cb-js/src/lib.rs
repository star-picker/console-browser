//! cb-js: 最小 JS 运行时（QuickJS）+ DOM/BOM 桥。
//!
//! 目标不是实现浏览器，而是让「靠 JS 渲染内容」的页面能被读出正文：
//! - 执行 inline / 外部脚本；
//! - 提供 document.title / document.body.innerText / document.querySelectorAll 的
//!   最小实现（后续接真实 DOM）；
//! - fetch 走 cb-net（保留 TLS 指纹与 cookie），不直连网络。

use anyhow::Result;

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

//! 登录墙 / 人机验证的识别。
//!
//! 目标：让上层（CLI 错误页、TUI 的「需要你」页、agent）知道「这页不是内容，是要人」。
//! 只做启发式判断，不猜账号、不自动提交表单。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginWallKind {
    Login,
    Captcha,
    TwoFactor,
    Unknown,
}

impl LoginWallKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LoginWallKind::Login => "login",
            LoginWallKind::Captcha => "captcha",
            LoginWallKind::TwoFactor => "two_factor",
            LoginWallKind::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginWall {
    pub kind: LoginWallKind,
    /// 人机验证的厂商（recaptcha / hcaptcha / turnstile / geetest）。
    pub provider: Option<String>,
    /// 给人类看的下一步建议。
    pub hints: Vec<String>,
    /// 触发判断的证据，便于排查误报。
    pub evidence: Vec<String>,
}

const LOGIN_PATH_HINTS: &[&str] = &[
    "/login",
    "/signin",
    "/sign-in",
    "/auth",
    "/sso",
    "/account/login",
];
const LOGIN_BODY_HINTS: &[&str] = &[
    "type=\"password\"",
    "type='password'",
    "name=\"otp\"",
    "two-factor",
    "two_factor",
    "verify your identity",
    "sign in to continue",
    "log in to continue",
];
const CAPTCHA_PROVIDERS: &[(&str, &str)] = &[
    ("recaptcha", "recaptcha"),
    ("hcaptcha", "hcaptcha"),
    ("turnstile", "turnstile"),
    ("geetest", "geetest"),
    ("nc_", "aliyun-nocaptcha"),
    ("cf-challenge", "cloudflare"),
    ("ddos-guard", "ddos-guard"),
];

fn lower(body: &str) -> String {
    body.to_ascii_lowercase()
}

/// 去掉脚本/样式内容：反爬标记经常只出现在 JS 里，扫描正文前先剥掉，避免误判。
fn strip_scripts_and_styles(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let lower = html.to_ascii_lowercase();
    let mut i = 0usize;
    while i < html.len() {
        let rest = &lower[i..];
        let tag = if rest.starts_with("<script") {
            Some("</script")
        } else if rest.starts_with("<style") {
            Some("</style")
        } else {
            None
        };
        match tag {
            Some(close) => {
                if let Some(end) = lower[i..].find(close) {
                    i += end;
                    // 跳过完整的闭合标签
                    if let Some(gt) = lower[i..].find('>') {
                        i += gt + 1;
                    } else {
                        i = html.len();
                    }
                } else {
                    i = html.len();
                }
            }
            None => {
                let ch_len = html[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                out.push_str(&html[i..i + ch_len]);
                i += ch_len;
            }
        }
    }
    out
}

/// 只做诊断：页面看起来像人机验证（不作为「需要人」的触发条件）。
pub fn detect_captcha(body: &str) -> Option<String> {
    let cleaned = lower(&strip_scripts_and_styles(body));
    for (marker, provider) in CAPTCHA_PROVIDERS.iter() {
        if cleaned.contains(marker) {
            return Some((*provider).to_string());
        }
    }
    None
}

/// 判定顺序：验证码 > 2FA > 登录。
pub fn detect_login_wall(
    final_url: &str,
    headers: &[(String, String)],
    body: &str,
    jar_has_session: bool,
) -> Option<LoginWall> {
    let url_lower = final_url.to_ascii_lowercase();
    // 只扫正文：脚本里的标记不算「要人」证据。
    let body_lower = lower(&strip_scripts_and_styles(body));
    let mut evidence: Vec<String> = Vec::new();

    let two_factor = body_lower.contains("two-factor")
        || body_lower.contains("two_factor")
        || body_lower.contains("name=\"otp\"")
        || body_lower.contains("verification code");
    let password_form = LOGIN_BODY_HINTS
        .iter()
        .any(|hint| body_lower.contains(&hint.to_ascii_lowercase()));
    let login_path = LOGIN_PATH_HINTS.iter().any(|p| url_lower.contains(p));
    let auth_header = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("www-authenticate") && !v.trim().is_empty());

    if two_factor && password_form {
        evidence.push("password form with 2FA marker".to_string());
    }
    if password_form {
        evidence.push("password input in body".to_string());
    }
    if login_path {
        evidence.push(format!("login-ish path: {url_lower}"));
    }
    if auth_header {
        evidence.push("www-authenticate header".to_string());
    }

    if two_factor && (password_form || login_path) {
        return Some(LoginWall {
            kind: LoginWallKind::TwoFactor,
            provider: None,
            hints: vec!["finish the second factor in the human assist window (e)".to_string()],
            evidence,
        });
    }
    if password_form || login_path || auth_header {
        let hints = if jar_has_session {
            vec![
                "a session cookie exists but this page still asks to log in; it may have expired"
                    .to_string(),
                "open the human assist window (e) to refresh it".to_string(),
            ]
        } else {
            vec![
                "open the human assist window (e) to log in".to_string(),
                "or paste cookies manually (c) if you already have them".to_string(),
            ]
        };
        return Some(LoginWall {
            kind: LoginWallKind::Login,
            provider: None,
            hints,
            evidence,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(html: &str) -> Option<LoginWall> {
        detect_login_wall("https://site.example/page", &[], html, false)
    }

    #[test]
    fn normal_page_is_not_a_login_wall() {
        assert!(
            parse("<html><body><article><h1>Docs</h1><p>hello</p></article></body></html>")
                .is_none()
        );
    }

    #[test]
    fn password_form_is_login() {
        let wall = parse("<form><input type=\"password\" name=\"pw\"></form>").expect("login");
        assert_eq!(wall.kind, LoginWallKind::Login);
        assert!(!wall.hints.is_empty());
    }

    #[test]
    fn captcha_alone_is_not_a_login_wall() {
        // 只有验证码、没有登录表单：不打断用户，只做诊断记录。
        let html = "<div class=\"g-recaptcha\" data-sitekey=\"x\"></div>";
        assert!(
            parse(html).is_none(),
            "captcha must not force human handoff"
        );
        assert_eq!(detect_captcha(html).as_deref(), Some("recaptcha"));
    }

    #[test]
    fn script_noise_does_not_trigger_detection() {
        let html =
            "<html><body><h1>docs</h1><script>var nc_='x'; grecaptcha=1;</script></body></html>";
        assert!(
            parse(html).is_none(),
            "markers inside scripts must be ignored"
        );
    }

    #[test]
    fn two_factor_detected() {
        let wall = parse("<form><input type=\"password\"><p>Enter your two-factor code</p></form>")
            .expect("2fa");
        assert_eq!(wall.kind, LoginWallKind::TwoFactor);
    }

    #[test]
    fn login_path_is_enough() {
        let wall =
            detect_login_wall("https://site.example/sso/start", &[], "", false).expect("login");
        assert_eq!(wall.kind, LoginWallKind::Login);
    }
}

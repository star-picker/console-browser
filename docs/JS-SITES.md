# JS 站实测记录（离线检视，持续更新）

> 目的：给「cb-js 要补到什么程度」提供依据。每条都记实测命令、结果、结论。
> 环境：Windows，本机代理未配置给 cb；网络可用性以当次实测为准。

## 1. GitHub 仓库首页（服务端渲染）

```powershell
cb --json fetch --fresh https://github.com/rust-lang/rust
```

- 结果：`ok: true`，`extract.chars = 10852`，`extract.paragraphs = 135`
- 结论：**服务端渲染的页面不需要 JS 引擎**，正文评分 + markdown 已经够用。

## 2. GitHub 文件树（JS 重）

```powershell
cb --json fetch --fresh https://github.com/rust-lang/rust/tree/master/src
```

- 结果：`ok: false`，`error.code = timeout`
- 详情：`all 3 profiles failed after 17s (chrome:timeout, firefox:timeout, safari:timeout)`，最内层是 `client error (Connect)`
- 结论：这次是**网络层连不上**（连接阶段超时），不是抽取失败；同一台机器抓仓库首页成功，所以与抽取器无关。需要换网络环境重测才能判断正文质量。

## 3. 百度首页（门户页，曾把整页压成一行）

```powershell
cb fetch --fresh https://www.baidu.com/ --to md
```

- 修复后：`card_title` 命中 0（JSON 数据转储被过滤）、`![](` 命中 0（空 alt 图标不输出）、导航行可读为 `新闻 hao123 地图 贴吧 视频 …`
- 结论：门户页结构渲染已可用；这类页面本来就没有多少正文，属预期。

## 4. 微信公众号文章（用户实机）

- 用户截图：TUI 正文可读、分段落、标题成行（46 行 / 563tok）
- 图片：正文里不显示（终端硬限制），走懒加载属性链抽到 URL；在 `cb webui` 与 `cb export --to html` 里可见
- 结论：文章类页面（含服务端渲染的公众号正文）**不需要 JS 引擎**。

## 5. 结论与下一步

- 目前**没有证据**表明「主流内容页必须跑 JS」；已知的两类失败：网络不可达、以及真正 SPA（数据靠 XHR 后渲染）。
- 因此 cb-js 的优先级仍低于：daemon/句柄 `@eN`（会话能力）与 AI 流式（体验）。
- 待重测：GitHub 文件树（换网络）、deepseek 分享页（需要真实分享链接）、任一 SPA 新闻站。

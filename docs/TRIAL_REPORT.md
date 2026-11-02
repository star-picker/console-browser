# 试用报告与不足清单

> 试用者：Hermes Agent（作为本工具的目标用户之一：给 AI 用的网页抓取/搜索后端）
> 版本：`cb 0.1.0.20261103.e682c7b`（debug build，2026-11-03）
> 环境：Windows 10 / git-bash / 无系统级代理（本机梯子在 127.0.0.1:7890，未给 cb 配）
> 日期：2026-11-03

---

## 一、结论先行

**能用，而且和我现在手上的网页抓取方案比是升级。**

```
web_extract   （Hermes 内置）  → 依赖 Firecrawl，没 API key 就直接报错
cb fetch      （本项目）       → 本地跑，零依赖，输出干净
```

**实测微信公众号、阮一峰博客、example.com 三类页面，正文提取质量都在可用线以上。**
**微信公众号那篇，我用 Playwright 需要手写选择器，`cb fetch` 一条命令就出结果。**

**但有几个问题会直接影响「AI 把它当可靠数据源」这件事。按严重程度排如下。**

---

## 二、P0 · 会让调用方拿到错误结论

### P0-1　搜索「成功但零结果」不可区分

**现象：**

```json
$ cb search "rust async runtime" --engine bing --limit 5 --json
{
  "ok": true,
  "data": { "engine": "bing", "query": "rust async runtime", "results": [] },
  "error": null
}
```

**`ok: true`、`results: []`，耗时 156 ms。**

**问题：** 调用方（AI / agent）无法区分下面三种情况：

```
① 真的没有结果
② SERP 解析失败（页面结构变了）
③ 引擎返回了拦截页 / 验证页 / 空壳页
```

**对 AI 来说，`ok:true + []` 会被读成「搜索过了，确实没有」——这是最危险的一种错误，因为它看起来是成功的。**

**建议：**

```json
"data": {
  "engine": "bing",
  "results": [],
  "empty_reason": "parse_failed" | "no_results" | "blocked",
  "http_status": 200,
  "bytes": 48321
}
```

**判定 `parse_failed` 的廉价办法：HTTP 200 + 字节数 > 10KB + 解析出 0 条 → 几乎一定是选择器失效。**

**至少让 `ok` 在 `parse_failed` 时为 `false`，或者引入 `ok:true, empty:true`。**

---

### P0-2　搜索引擎没有 fallback

**现象：**

```json
$ cb search "rust async runtime" --engine ddg --json
{
  "ok": false,
  "error": {
    "code": "network",
    "message": "network error: ... https://html.duckduckgo.com/html/?q=...: client error (Connect)",
    "retryable": true
  },
  "meta": { "elapsed_ms": 21053 }
}
```

**问题：**

```
① 一个引擎挂了就整个失败，不试其他引擎
② 白等 21 秒才返回失败
```

**本项目在 `challenge` 上已经做了「403 → 换指纹重试 3 轮」的阶梯策略，但在搜索引擎上完全没有对应设计。**

**建议：**

- 提供 `--engine auto`：按 `gh → bing → ddg → arxiv` 顺序尝试，首个非空结果即返回
- 连接失败类错误（`Connect`）不应该等满超时，3-5 秒连不上就该换下一个
- 返回里带上 `attempts: [{engine, ok, error}]`，让调用方知道你试过什么

---

### P0-3　空结果时不报告「试过哪些引擎」

**与 P0-2 相关但与 P0-1 不同：** 即使单引擎也能用，调用方也不知道「这个查询在本工具上到底可不可靠」。

**`cb search` 的 `meta` 里有 `engine / cached / profile`，但没有 `http_status / bytes / parsed_count`。**

**建议统一补上这三个字段，让「零结果」可自证。**

---

## 三、P1 · 影响可用性和体验

### P1-1　懒加载图片的 `data-src` 未被提取（通用问题）

**实测案例：** 微信公众号文章

```
$ cb fetch "https://mp.weixin.qq.com/s/..." --to md
→ 正文 1798 字（比 Playwright 手动抓的 602 字更全）
→ 图片 0 张
```

**同页用 Playwright 抓，读出 15 张图片 URL。**

**根因（通用形态）：**

**大量站的 `<img>` 不写 `src`，而是把真地址放在 `data-src` / `data-original` / `data-lazy-src` / `data-actualsrc`，`src` 里是一张 1×1 占位图或 base64 微图。**

**如果只取 `src`，结果是：**

```
① 一张图都拿不到（占位图被过滤掉）
② 或者拿到一堆无效的占位 URL
```

**受影响的站（不止微信）：**

```
微信公众号     data-src
知乎            data-original
简书            data-src
掘金            data-src
Medium          data-src
大量 WordPress data-lazy-src
各种 CDN 图床   data-original / data-actualsrc
```

**建议：**

**按优先级取第一个非占位的属性：**

```
srcset（取最大尺寸的一项）
  > data-src
  > data-original
  > data-lazy-src
  > data-actualsrc
  > data-original-src
  > src（最后兜底）
```

**占位图判定（同时满足即跳过）：**

```
① 尺寸 ≤ 2×2（若有 width/height 属性）
② 或 URL 含 placeholder / blank / loading / spacer / 1x1 / transparent
③ 或 data: URI 且长度 < 200 字节
```

**输出侧：** markdown 里保留 `![alt](url)`，同时 `--json` 时给 `data.images: [{url, alt, width, height, lazy: true}]`，让调用方知道这张图是不是懒加载来的。

**为什么这条是通用坑而不是站点评坑：** 懒加载是 2020 年之后绝大多数内容站的默认做法，任何「HTML → markdown」的转换器只要只读 `src` 就会漏图，且**漏得静默**（没人报错，就是没图）。

---


### P1-2　`browse_url` 不返回「正文占比」

**现象：** 抓到微信公众号时，markdown 里混入了：

```
在小说阅读器读本章
去阅读
在公众号小说中沉浸阅读
预览时标签不可点
不喜欢
Scan to Follow
```

**问题：** 这些是页面 chrome（控件文案），不是正文。

**AI 拿到之后会把这些当成内容的一部分去理解。**

**建议：** 正文容器评分（`cb-dom` 已经有 `正文容器评分` 的能力）应该在结果里暴露一个置信度：

```json
"meta": { "extract": { "container_score": 0.87, "stripped_ratio": 0.63 } }
```

**`stripped_ratio` 是「被剥掉的文本 / 总文本」，低于某阈值（比如 0.2）说明可能抓错了容器，应该警告。**

---

### P1-3　没有「正文长度下限」保护

**现象：** `cb fetch https://example.com --to md` 输出：

```
This domain is for use in documentation examples without needing permission.
This is not a service; avoid relying on it for testing and monitoring purposes.
```

**这是对的**（那页正文就这么多），**但问题在于它和「抓到了但解析失败只留一句话」长得一模一样。**

**建议：** 输出 `extract.chars` 和 `extract.paragraphs`，让调用方自己判断。低于阈值（如 50 字）时在 stderr 打个 warning。

---

### P1-4　`--to html` 的导出没有验证入口

**`cb export --to html` 做了很多正确的事**（剥 script / 外链 link / 内联 on* / HTML 转义），**但没有命令能让我确认「导出的文件真的自包含」。**

**建议：** `cb export --verify`，检查：

```
① 剩余 <script> 数量 = 0
② 剩余外部资源引用（src/href 指向 http）数量
③ 总字节数
```

**对 AI 来说，「我导出的文件能不能离线打开」是一个需要机器可读答案的问题。**

---

### P1-5　缓存命中不区分「新鲜」和「陈旧」

**`meta.cached` 是布尔值，但 `cache_entries` 表里有 `expires_at`。**

**建议：** `meta.cache: { "hit": true, "age_s": 3600, "stale": false }`

**理由：** AI 拿旧数据下结论是有风险的，需要知道数据多老。

---

### P1-6　`tools --json` 的 `description` 为空

**现象：**

```json
$ cb tools --json
  browse_url         （空）
  search_web         （空）
  history_search     （空）
  selftest_status    （空）
```

**但 `hermes mcp test` 里能看到 description 是有内容的**（"Fetch a page and return AI-ready content (markdown by d..."）。

**问题：** `cb tools --json` 的字段名可能不是 `description`，或者 CLI 输出没带上。

**这不影响功能，但影响「工具注册表作为唯一真相源」这个设计承诺。**

---

## 四、P2 · 打磨项

### P2-1　DDG 在无代理环境下 21 秒才失败

**`Connect` 类错误应该在 3-5 秒内判定，不该等满 timeout。**
**建议给网络层一个 `connect_timeout` 与 `read_timeout` 分开的配置。**

### P2-2　`history --limit` 输出是 TSV 但无表头

```
1793638685	gh: tokio	https://api.github.com/search/repositories?q=tokio&per_page=3
```

**时间戳是 epoch 秒（`1793638685` = 2026-11-03），机器可读但人不可读。**
**建议 `--json` 时给 ISO8601，或加 `--human` 转本地时间。**

### P2-3　`selftest` 的 skipped 项区分度不够

```
tls     skipped  （需要联网 oracle）
js      skipped  （M3 才有）
llm     skipped  （未配置）
```

**三种完全不同的原因都叫 `skipped`。**
**建议 status 分成 `skipped_offline` / `not_implemented` / `not_configured`。**

**理由：** 我作为调用方，看到 `skipped` 不知道是该去配 LLM 还是单纯等 M3。

### P2-4　没有 `--timeout` 全局参数

**`fetch` 一个慢站可能挂很久，调用方没有统一的刹车。**
**建议顶层加 `--timeout <sec>`，对所有网络操作生效。**

### P2-5　错误码只有四种

**`network / challenge / needs_auth / timeout`。**

**建议补：`parse`（解析失败）、`empty`（页面无正文）、`blocked`（被拦截页）。**
**这三个正是 P0-1 需要的区分。**

---

## 五、对比测试记录

| 目标 | 命令 | 结果 | 评价 |
|---|---|---|---|
| example.com | `cb fetch --to md` | 2 句 | ✓ 正确（本来就这么少） |
| 阮一峰博客 | `cb fetch --to md` | 标题/作者/日期/引用块/图片链接全对 | ✓✓ 优秀 |
| 微信公众号 | `cb fetch --to md` | 正文 1798 字（比 Playwright 多 3 倍），**图片 0 张** | ✓ 文字优秀 / ✗ 图片全丢（P1-1） |
| GitHub 搜索 | `cb search --engine gh` | 3 条，带 ★ 数 | ✓✓ 优秀（走 REST） |
| Bing 搜索 | `cb search --engine bing` | `ok:true` 空结果 | ✗ P0-1 |
| DDG 搜索 | `cb search --engine ddg` | 21s 后 network error | ✗ P0-2 |
| 自检 | `cb selftest --json` | 8 项，5 ok / 3 skipped | ✓ 结构好，P2-3 可改进 |
| 工具表 | `cb tools --json` | 4 个工具，描述为空 | ✗ P1-5 |
| 数据库 | `cb db schema` | 表 + 列 + 行数 | ✓ 好用 |
| 版本 | `cb --version` | `0.1.0.20261103.e682c7b` | ✓ 方案生效 |

---

## 六、一句话总结

**正文提取和抓取这一块，比大多数同类工具做得好；**

**搜索这一块，可靠性还不够当「唯一数据源」——因为它的失败会长得像成功。**

**把 P0 三条修完，我就能把它当成默认的网页后端长期用。**

---

## 七、给作者的具体建议顺序

```
1. cb search 补 empty_reason + http_status + bytes          （半小时）
2. cb search --engine auto + 连接失败快速切换                （一小时）
3. 图片属性优先级链 + 占位图判定                             （一小时）  ← 新增
4. 错误码补 parse / empty / blocked                          （半小时）
5. meta.extract 补 container_score / chars / paragraphs      （一小时）
6. meta.cache 从布尔改成对象（hit / age_s / stale）           （半小时）
7. cb tools --json 带上 description                          （二十分钟）
8. selftest status 细分                                      （二十分钟）
```

**前三条修完，这个工具对我来说就从「可用」变成「可信」。**

**关于第 3 条排这么前：** 它虽然被归类在 P1，但对「抓文章」这个主用途来说，**丢图的伤害比搜索偶然空结果更常见、更不可补救**——文字可以再抓一次，图不在原文里就永远找不回来了。人物报道、学校喜报、教程配图，全在图片里。

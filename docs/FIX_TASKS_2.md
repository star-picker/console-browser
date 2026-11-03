# cb 修复任务单（第二轮）

> 来源：`docs/TRIAL_REPORT_2.md`（Hermes Agent 全功能冒烟，48 项）
> 版本基线：`cb 0.1.0.20261103.7f2da8f`
> 目标：修完之后，cb 可以作为 AI 的默认网页后端长期运行
> 要求：**每修一条，补一个测试；每一条单独 commit**

---

## 快速索引

| 编号 | 标题 | 严重度 | 预计工时 | 状态 |
|---|---|---|---|---|
| FIX-1 | `--to md` 输出里没有任何图片 | ★★★ | 1h | [ ] |
| FIX-2 | `fetch` 无重试、无 profile 阶梯 | ★★★ | 2h | [ ] |
| FIX-3 | `meta.cache.age_s` 恒为 null（真值在 data 里） | ★★ | 20m | [ ] |
| FIX-4 | `extract.container_score` 恒为 null | ★★ | 30m | [ ] |
| FIX-5 | arXiv 429 被标成不可重试 | ★★ | 20m | [ ] |
| FIX-6 | DNS 解析失败被标成可重试 | ★ | 20m | [ ] |
| FIX-7 | `--scope` 命中与否无回执 | ★ | 20m | [ ] |
| FIX-8 | `webui` 首页 `?url=` 参数无效 | ★ | 20m | [ ] |
| FIX-9 | `bench` 失败时不留部分结果 | ★ | 20m | [ ] |

**合计约 5 小时。**

---

# FIX-1　`--to md` 输出里没有任何图片　★★★

## 现象

```
cb fetch https://mp.weixin.qq.com/s/CldcMVEuUcze48A0KXOHfQ --to md  →  ![..](..) 数量 = 0
cb fetch https://www.ruanyifeng.com/blog/2024/01/weekly-issue-287.html --to md  →  0
cb fetch https://en.wikipedia.org/wiki/Rust_(programming_language) --to md  →  0
cb fetch https://www.bbc.com/news --to md  →  0
```

**四个站点全部 0 张，所以不是懒加载问题。**

## 对照证据

**同页 `--to html` 的输出里有正常的 img：**

```html
<img src="https://cdn.beekka.com/blogimg/asset/202401/bg2024011103.webp" alt="" title="" />
<img src="https://cdn.beekka.com/blogimg/asset/202401/bg2024011401.webp" alt="" title="" />
```

**同页 `--json` 的输出里有图片数组：**

```json
"images": [
  {"url": "https://mmbiz.qpic.cn/mmbiz_png/RGLWxw9OCl5...", "alt": "", "lazy": true},
  ... 共 15 条
]
```

**所以：提取层拿到了图片，markdown 渲染层把它们丢了。**

## 影响

**这是唯一挡住「AI 用 cb 写带图报告」的问题。**

**我现在要引用一张图，只能：**

```
① cb fetch --json → 拿 images[]
② 手工按顺序插回 markdown
③ 但 images[] 和正文段落没有对应关系，插不准
```

**结果：要么没图，要么图和文对不上。**

## 修复要求

**① markdown 渲染时，把图片按文档原始位置插回去。**

```
<p>文字段落</p>
<img src="a.png">
<p>下一段</p>
        ↓
文字段落

![a.png](https://.../a.png)

下一段
```

**② 如果段落里有多张图，按出现顺序逐个输出，各自占一行。**

**③ 图片下面如果还有 `<figcaption>`，作为 `*caption*` 单独一行跟在图后面。**

**④ `alt` 为空时用文件名兜底，不要输出 `![](url)`（有些渲染器会当成装饰图跳过）。**

**⑤ 如果是有意不发图（担心 token），至少给个开关：**

```bash
--images           强制输出图片（默认开）
--no-images        不输出图片
```

**⑥ `--max-tokens` 截断时，图片也算 token（一张图约 20-40 tok，具体你定），但不要在截断时把图删掉只留文字——宁可少截一段文字。**

## 测试要求

```
① 微信公众号 → md 里图片数 == json 里 images 长度
② 阮一峰博客 → md 里图片数 > 0，且顺序与 html 一致
③ alt 为空的图 → 输出里有文件名兜底
④ --no-images → 图片数 = 0
⑤ 图片和正文段落交错时，位置正确（用 example 页构造一个 fixture）
```

---

# FIX-2　`fetch` 无重试、无 profile 阶梯　★★★

## 现象

**同一 URL、同一台机器、连续跑三次，三种结果：**

```
--fresh 第1次    ok        4579ms
--fresh 第2次    ok        4056ms
--fresh 第3次    timeout  30021ms

--profile chrome  第1次  timeout  30012ms
--profile chrome  第2次  ok       15832ms

--profile firefox 第1次  network error 15782ms
--profile firefox 第2次  timeout  30017ms

--profile safari  第1次  timeout  30021ms
--profile safari  第2次  timeout  30009ms
```

**对照：缓存命中时 69ms，完全稳定。**

## 诊断

**三层原因：**

**① 环境（占一半）**
这台机器访问境外站慢且抖，30 秒超时确实不够。这不是 cb 的错。

**② 工具（占一半）**
- 失败后不重试
- profile 失败后不换下一个
- 调用方无法知道「试过什么」（`search` 已经有 `attempts` 了，`fetch` 没有）

**③ 疑点：`--profile safari` 两次都满超时**
如果 Safari 的 TLS 指纹在某些站上会被降速或挂起，那 `--profile safari` 不是「换个指纹再试」，而是「换个指纹更容易挂」。

## 修复要求

**① 连接阶段和读取阶段分开超时**

```
connect_timeout   5s（连不上就立刻失败，不要等满 30s）
read_timeout      25s
total_timeout     30s（默认，可 --timeout 覆盖）
```

**② 失败自动重试（指数退避）**

```
第 1 次失败 → 退避 500ms  → 重试
第 2 次失败 → 退避 1500ms → 重试
第 3 次失败 → 报错
```

**只对 `network` / `timeout` 重试；`not_found` / `challenge_unsolved` 不重试。**

**③ profile 阶梯**

```
用户指定 --profile X 时：
  第 1 次用 X
  失败后 → chrome → firefox → safari（跳过 X 本身）
  全部失败才报错

用户不指定时：
  chrome → firefox → safari
```

**④ meta 里报 attempts**

```json
"meta": {
  "attempts": [
    {"profile": "chrome",  "ok": false, "ms": 30012, "error": "timeout"},
    {"profile": "firefox", "ok": true,  "ms": 4210,  "error": null}
  ]
}
```

**⑤ 错误信息里带总耗时和尝试次数**

```
"message": "all 3 profiles failed after 92s (chrome:timeout, firefox:timeout, safari:timeout)"
```

## 测试要求

**用 `--replay` 造三个 cassette（成功/失败/超时），测：**

```
① 第 1 个失败 → 自动换第 2 个 → 成功 → attempts 长度 2
② 三个都失败 → 报错 → attempts 长度 3
③ not_found 不重试 → attempts 长度 1
④ connect_timeout 5s 生效（用不可达 IP 测）
```

---

# FIX-3　`meta.cache.age_s` 恒为 null　★★

## 现象

```json
"meta": {
  "cache": { "hit": true, "age_s": null, "stale": false }
},
"data": {
  "cached": true,
  "cache_age_s": 131
}
```

**同一个事实表达了两遍：一处 null，一处 131。**

## 为什么这是问题

**我作为调用方，会优先读 `meta.cache.age_s`——它在 meta 里，位置更「正式」，字段名更语义化。**

**读到 null，我就判断不出这份数据有多旧。**

**而对 AI 来说，「数据多旧」是决定要不要采信的关键信息。**

## 修复要求

```
① meta.cache.age_s 填真实值（就是 data.cache_age_s 的值）
② data.cache_age_s 保留（向后兼容）或删掉（如果还没有外部依赖）
③ 未命中缓存时 age_s 应该是 null（这个是对的，保持）
```

**测试：**

```
① 首次请求 → hit:false, age_s:null
② 1 秒后再请求 → hit:true, 0 <= age_s <= 3
③ meta.cache.age_s == data.cache_age_s（如果保留）
```

---

# FIX-4　`extract.container_score` 恒为 null　★★

## 现象

```json
"extract": { "chars": 648, "paragraphs": 21, "container_score": null }
```

**字段加了，但从来没有值。**

## 为什么这是问题

**我第一轮提这条，要的是「判断有没有抓错容器」。**

**`chars` / `paragraphs` 只能告诉我「抓到了多少」，不能告诉我「抓得对不对」。**

**一个抓错容器的页面，chars 也可能是 648。**

**而恒为 null 的字段，会让调用方以为「这次特别差」，其实是「从来没实现」。**

## 修复要求

**二选一：**

**方案 A（推荐）：实现它**

```
container_score = 选中容器的正文字数 / 页面总正文字数
取值范围 0.0 - 1.0

判读：
  > 0.7  抓得很准（选中容器几乎就是全文）
  0.3-0.7 一般（可能漏了侧栏或评论）
  < 0.3  可疑（可能抓错了容器，或者页面本身是列表页）
```

**方案 B：删掉这个字段**

**如果短期不打算实现，就删掉。放一个永远 null 的字段比不放更糟。**

**我倾向 A——`cb-dom` 已经有正文容器评分的能力，把这个分数透出来应该是顺手的事。**

## 测试要求

```
① 单篇文章页 → score 在 0.7-1.0
② 列表页（首页）→ score 明显偏低，或明确标注"非文章页"
③ 抓错容器的 fixture → score < 0.3
```

---

# FIX-5　arXiv 429 被标成不可重试　★★

## 现象

```json
"error": {
  "code": "challenge_unsolved",
  "message": "arxiv blocked with http 429",
  "retryable": false
}
```

## 为什么这是问题

**429 = Too Many Requests，是「你太快了」，不是「你解不开挑战」。**

**标成 `retryable: false` 之后，调用方会放弃，但其实等几秒就能过。**

## 修复要求

```
① 429 的错误码单独出来：rate_limited
② retryable: true
③ 带上建议等待时长：retry_after_s（从 Retry-After 头读，没有就给 5）
④ 连续 429 时按指数退避（5s → 15s → 45s）
```

**新的错误信封：**

```json
"error": {
  "code": "rate_limited",
  "message": "arxiv rate limited (429), retry after 5s",
  "retryable": true,
  "retry_after_s": 5,
  "needs_user": null
}
```

## 测试要求

```
① 429 响应 → code=rate_limited, retryable=true, retry_after_s 有值
② Retry-After: 30 头 → retry_after_s=30
③ 无 Retry-After 头 → retry_after_s=5（默认）
```

---

# FIX-6　DNS 解析失败被标成可重试　★

## 现象

```json
"error": {
  "code": "network",
  "message": "network error: error sending request for url (https://this-domain-does-not-exist-xyz.com)",
  "retryable": true
}
```

## 为什么这是问题

**域名不存在。重试一万次结果都一样。**

**标 `retryable: true` 会让上层做无用的退避重试，浪费时间。**

## 修复要求

```
① 区分 dns_error 和 network
② dns_error: retryable = false
③ 错误信息里带域名，方便排查
```

```json
"error": {
  "code": "dns_error",
  "message": "DNS lookup failed for this-domain-does-not-exist-xyz.com",
  "retryable": false
}
```

**判据：底层错误链里出现 `dns error` / `failed to lookup address` / `Name or service not known` 就归到 dns_error。**

## 测试要求

```
① 不存在的域名 → dns_error, retryable=false
② 存在的域名但连接被拒 → network, retryable=true
```

---

# FIX-7　`--scope` 命中与否无回执　★

## 现象

```
无 scope              → 12118 字
--scope article       → 12118 字（一模一样）
--scope .content      → 0 字（阮一峰那页没有 .content）
```

**行为是对的（`article` 命中了根容器，`.content` 没命中返回空）。**

**但调用方无法区分「scope 命中但没缩小」和「scope 没命中」。**

## 修复要求

**meta 里加 scope 回执：**

```json
"extract": {
  "chars": 12118,
  "paragraphs": 21,
  "container_score": 0.95,
  "scope": {
    "selector": "article",
    "matched": true,
    "chars_before": 12118,
    "chars_after": 12118
  }
}
```

**没命中时：**

```json
"scope": { "selector": ".content", "matched": false }
```

**建议：没命中时打一条 stderr warning（AI 看不到 stderr，但人能看见）。**

## 测试要求

```
① scope 命中且缩小 → matched=true, chars_after < chars_before
② scope 命中但没缩小 → matched=true, chars_after == chars_before
③ scope 没命中 → matched=false
```

---

# FIX-8　`webui` 首页 `?url=` 参数无效　★

## 现象

```
cb webui --port 7879
GET /?url=https://example.com  →  200，但返回的是空首页
POST /api/navigate {"url":"https://example.com"}  →  正常工作
```

## 修复要求

**二选一：**

**方案 A：支持 `?url=`（推荐）**
首页加载时读 query 参数，自动调一次 navigate。

**方案 B：明确不支持，文档写清楚**
在 webui 页面的输入框 placeholder 里说明「直接输入网址」。
**不要让人试了参数却静默失败。**

## 测试要求

```
① GET /?url=https://example.com → 返回的 HTML 里包含该 URL 或已导航的内容
② 无参数 → 空首页
```

---

# FIX-9　`bench` 失败时不留部分结果　★

## 现象

```
cb bench https://... --n 2 --json
→ code=7 (timeout)，没有任何耗时数据
```

**但其实第 1 次跑成功了（只是超时发生在第 2 次或后续）。**

## 修复要求

```
① 每完成一次就记录 {i, ok, ms, error}
② 中途失败时，返回已完成样本 + 失败样本
③ 顶层 ok:false 但 data.samples 有内容
```

```json
{
  "ok": false,
  "data": {
    "samples": [
      {"i": 0, "ok": true,  "ms": 4210,  "error": null},
      {"i": 1, "ok": false, "ms": 30012, "error": "timeout"}
    ],
    "completed": 2,
    "planned": 3,
    "p50_ms": null,
    "error": "aborted at sample 2"
  }
}
```

**理由：bench 是用来做性能守门的，失败时更需要知道「成功那几次多快」。**

## 测试要求

```
① 全部成功 → samples 长度 == n
② 中途失败 → samples 里既有 ok:true 也有 ok:false
③ 第 1 次就失败 → samples 长度 1，ok:false
```

---

# 通用要求

## 每一条都要

```
① 一个测试（能用 --replay cassette 的优先）
② cargo test 全绿
③ clippy -D warnings 无告警
④ 单独 commit，message 用 fix(...) 前缀
```

## 修完自检清单

```bash
# 1. 全部测试
cargo test --workspace

# 2. lint
cargo clippy --workspace -- -D warnings
cargo fmt --check

# 3. 冒烟
cd D:/hermes-tools && python cb_smoke.py

# 4. 手工验证 FIX-1（最重要）
cb fetch "https://www.ruanyifeng.com/blog/2024/01/weekly-issue-287.html" --to md | grep -c '!\['
# 期望：> 0

# 5. 手工验证 FIX-3
cb fetch "https://example.com" --json | grep -A3 '"cache"'
# 期望：第二次请求时 age_s 不是 null
```

---

# 附：不在本单内的已知事项

**以下问题已在第一轮报告（`TRIAL_REPORT.md`）中记录，但本轮未复测/未列入修复：**

```
P1-4  export 无 --verify 入口（自包含性验证）
P2-2  history --limit 输出 TSV 无表头、时间戳是 epoch
P2-3  selftest 的 skipped 不区分原因（offline / not_implemented / not_configured）
```

**这三条优先级低于本单，可以先不修。**

---

# 附：验收标准

**修完之后，我会用同一个脚本重跑一遍：**

```bash
cd /d/hermes-tools && python cb_smoke.py
```

**验收线：**

```
① 48 项冒烟通过率 >= 44（扣掉纯网络抖动的 4 项）
② FIX-1 的四个站 md 图片数都 > 0
③ FIX-2 的 profile 阶梯能在 cassette 上复现
④ 无新增回归
```

**过了这条线，我就把 cb 接成 Hermes 的默认网页后端（MCP 已接好，只等这版稳定）。**

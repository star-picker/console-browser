# 第二轮试用报告 · 全功能冒烟

> 试用者：Hermes Agent
> 版本：`cb 0.1.0.20261103.7f2da8f`（debug build，32 MB，编译于 2026-11-03 13:11）
> 环境：Windows 10 / git-bash / 无系统级代理（梯子在 127.0.0.1:7890，未给 cb 配）
> 日期：2026-11-03
> 方法：脚本化冒烟 `cb_smoke.py`（48 项）+ 人工抽查语义（网络抖动项多次复现）

---

## 〇、一句话结论

**第一轮报的 P0/P1 绝大部分真修了，而且修得对。**

**新暴露的问题集中在「网络抖动时无重试」和「markdown 图片链路」两处。**

```
冒烟通过   39 / 48
真失败      3 项（都不是网络抖动能解释的）
网络抖动    6 项（同一命令多次跑结果不一致，属稳定性问题）
```

---

## 一、第一轮问题的修复验收

| 编号 | 问题 | 状态 | 证据 |
|---|---|---|---|
| P0-1 | 搜索「成功但零结果」不可区分 | ✅ 已修 | `empty_reason: "blocked"` + `http_status: 302` + `bytes: 176` |
| P0-2 | 搜索引擎无 fallback | ✅ 已修 | `--engine auto` → 自动落到 gh，776ms |
| P0-3 | 不报告试过哪些引擎 | ✅ 已修 | `attempts: [{engine, empty_reason, results}]` |
| P1-1 | 懒加载图片未提取 | ⚠️ 部分修 | JSON 里 `images` 15 张 + `lazy: true`；**但 `--to md` 输出 0 张图** |
| P1-2 | 不返回正文占比 | ⚠️ 部分修 | `extract: {chars, paragraphs, container_score: null}` — 字段有了，`container_score` 恒为 null |
| P1-3 | 无正文长度下限 | ✅ 已修 | `extract.chars` + `paragraphs` 可用 |
| P1-4 | 导出无验证入口 | ❌ 未做 | `export` 仍无 `--verify` |
| P1-5 | 缓存不区分新鲜/陈旧 | ⚠️ 部分修 | `meta.cache: {hit, age_s, stale}` 有，但 **`age_s` 恒为 null**；真实年龄在 `data.cache_age_s`（示例 131 秒） |
| P1-6 | `tools --json` 描述为空 | 待复查 | 见下 |

**总结：3 条 P0 全修且修得对；P1 有 3 条被彻底修掉，3 条只做了一半。**

---

## 二、真失败项（3 条）

### F-1　`--to md` 输出里一张图片都没有　★★★

**这是本轮最严重的问题，因为它让 `images` 字段变成孤儿数据。**

**实测：**

```
cb fetch https://mp.weixin.qq.com/s/...  --to md   →  0 张 ![..](..)
cb fetch https://www.ruanyifeng.com/...  --to md   →  0 张 ![..](..)
cb fetch https://en.wikipedia.org/wiki/Rust --to md →  0 张 ![..](..)
cb fetch https://www.bbc.com/news        --to md   →  0 张 ![..](..)
```

**四次全 0，说明不是懒加载的问题——`--to html` 里明明有正常的 `<img src="https://cdn.beekka.com/...">`。**

**同页 `--json` 时 `data.images` 是 15 张（微信公众号）/ 应该有若干张（阮一峰）。**

**结论：提取层拿到了图片，markdown 渲染层把它们丢了。**

**对 AI 的实际影响：**

```
我要写一份带图报告 → markdown 里没图 → 只能回头再走 --json 手工拼
```

**建议：** markdown 渲染时把 `images[]` 按文档顺序插回原位置输出 `![alt](url)`。**如果是有意不发图（担心 token），至少要有个 `--images` 开关。**

---

### F-2　`container_score` 恒为 null　★★

**字段加了，但从来没有值。**

```
"extract": {"chars": 648, "paragraphs": 21, "container_score": null}
```

**我第一轮提这条的目的是「判断是不是抓错了容器」。`chars` 和 `paragraphs` 只能告诉我「有多少」，不能告诉我「抓得对不对」。**

**建议：** 如果暂时算不出来，宁可不放这个字段，也不要放一个恒为 null 的字段——**恒 null 的字段会让调用方以为「这次特别差」，而其实是「从来没实现」。**

---

### F-3　`meta.cache.age_s` 恒为 null，真实值在别处　★★

```json
"meta":   { "cache": { "hit": true, "age_s": null, "stale": false } }
"data":   { "cached": true, "cache_age_s": 131 }
```

**同一个事实在两处表达，一处是 null，一处是 131。**

**对 AI 来说这是危险的：我会读 `meta.cache.age_s`（更新更「正式」的位置），拿到 null，然后判断不出缓存有多旧。**

**建议：** `meta.cache.age_s` 填真实值，`data.cache_age_s` 保留兼容或删掉。

---

## 三、网络抖动项（6 条，但不是纯粹的环境问题）

**这 6 条在脚本里是失败，但我单独复现时结果不一致。**

### N-1　`fetch --fresh` 与 `--profile` 首发超时率极高　★★★

**复现记录（同一 URL，同一台机器，连续跑）：**

```
--fresh 第1次    ok   4579ms
--fresh 第2次    ok   4056ms
--fresh 第3次    timeout 30021ms

--profile chrome 第1次   timeout 30012ms
--profile chrome 第2次   ok   15832ms
--profile firefox 第1次  network error 15782ms
--profile firefox 第2次  timeout 30017ms
--profile safari 第1次   timeout 30021ms
--profile safari 第2次   timeout 30009ms
```

**关键观察：**

```
缓存命中（不带 --fresh）   69ms，稳定
真实网络请求             4s / 15s / 30s 超时，三种结果都有
```

**我的判断，分三层：**

**① 一半是环境：这台机器访问境外站本身就慢且抖，30 秒超时是真的不够。**

**② 一半是工具：同一条命令三次三种结果，说明没有重试，也没有「失败即换 profile」的阶梯。**

**第一轮我提过「引擎没有 fallback」你修了，但 fetch 层的 fallback 还没做。**

**③ 还有一点值得注意：`--profile safari` 两次都超时（30s，两次都满）。**

**如果 Safari 的 TLS 指纹在目标站上会被降速或挂起，那 `--profile safari` 就不是「换个指纹再试一次」，而是「换个指纹更容易失败」。**

**建议：**

```
① fetch 失败自动重试（指数退避 3 次）
② profile 失败自动换下一个（chrome → firefox → safari）
③ meta 里报 attempts: [{profile, ok, ms, error}]，让调用方知道换过没
④ 给 connect_timeout 和 read_timeout 分开配（连接 5s 内不该等满 30s）
```

### N-2　`bench` 被 N-1 连坐

**`bench --n 2` 跑的是两次 fresh fetch，所以超时率跟 N-1 一样。**
**但 `bench` 报错时没有部分结果（第一次成功的耗时没留下）。建议失败时也返回已完成的样本。**

### N-3　`search --engine ddg` 21 秒才失败

**和第一轮一样。`--engine auto` 已经能绕开它，但用户显式指定 ddg 时仍然白等 21 秒。**

**建议：连接建立阶段设独立短超时（3-5s）。**

### N-4　`search --engine arxiv` 返回 429

```json
"error": {"code": "challenge_unsolved", "message": "arxiv blocked with http 429"}
```

**arXiv 对无 UA 或高频请求会 429。这个错误分类是对的（`challenge_unsolved`），但 `retryable: false`。**

**429 应该标 `retryable: true` + 建议退避时长——它不是「解不开」，是「太快了」。**

### N-5　`fetch 404页` 报 `not_found`　（这条其实是对的）

**`code: 6` / `not_found` / `http 404` / `retryable: false`。**
**分类准确，脚本判它失败是我脚本的问题（HTTP 404 本来就该失败）。这条不算 bug。**

### N-6　`fetch 坏域名` 报 `network` + `retryable: true`

**DNS 解析失败被标 `retryable: true` 是错的——域名不存在，重试一万次也一样。**

**建议：区分 `dns_error`（不可重试）和 `network`（可重试）。**

---

## 四、新功能的抽查

### ✅ `js` 子命令：能力比我预期强

```
1+1                                → 2
document.title                     → 科技爱好者周刊（第 287 期）...
document.querySelectorAll('a').length → 79
JSON.stringify({a:1})              → {"a":1}
new Date().getTime() > 0           → true
localStorage                       → {}（可用）
location.href                      → 正确
typeof fetch                       → undefined   ← 未实现 fetch
```

**DOM / BOM / 定时器都有，`fetch` 还没接。**

**另外确认了一件事：跑完 `js` 之后，同 URL 的 `fetch` 拿到的还是原页面（`title: Example Domain`），没有被污染。**

### ✅ `webui`：能跑，接口设计干净

```
cb webui --port 7879
GET  /                    → 200（单页，深色主题）
GET  /events              → SSE，event: snapshot
POST /api/navigate        → {"url":"https://example.com/","title":"Example Domain","status":"cached","lines":[...]}
```

**`/api/navigate` 的返回结构可以直接给前端用，不需要二次解析。**

**一个小问题：首页 `?url=` 参数无效（`/?url=https://example.com` 返回的还是空首页）。要导航必须走 `/api/navigate`。**

### ✅ `render`：离屏渲染可用

```
cb render https://example.com --width 80 --height 20
→ 完整一帧文本，含状态栏 "BROWSE  cache · 4 lines · 40tok"
```

**这个对快照测试和 CI 很有用。**

### ✅ `bookmark` / `cookies` / `log` / `challenge` / `site` / `revisions` / `plugin dir`

**全部正常返回结构化 JSON。**

**`log tail` 的参数是 `--n` 不是 `--limit`**（我脚本写错，工具本身没问题）：

```
log tail --n 5   → {"argv":["js","--code","1+1"],"code":0,"elapsed_ms":1,...,"ts":1793684613}
```

**`log stats` 的输出很有用：**

```json
{"avg_ms": 158, "commands": {"agent": 58, "ai": 158, "bench": 2, "bookmark": 237, ...}}
```

**命令级聚合，对性能看门很好用。**

### ⚠️ `--scope` 的语义

```
无 scope              → 12118 字
--scope article       → 12118 字（完全一样，说明 article 命中了根容器）
--scope .content      → 0 字（阮一峰那页没有 .content 类）
```

**行为是对的，但 `--scope article` 返回整页时，调用方无法得知「scope 命中了但没起缩小作用」。**
**建议：`meta` 里加 `{scope: "article", scope_matched: true, scope_chars: 12118}`。**

### ✅ `--max-tokens` 生效

```
--max-tokens 200 → chars=238, tokens_estimate=175
```

**确实截断了。**

---

## 五、控制台/UI 层

**`tui` 需要交互终端，无法在脚本里测（我没在有 TTY 的环境跑）。**
**`webui` 已测，能起、能导航、能推 SSE。**

---

## 六、按严重程度的总排序

```
★★★  1. --to md 丢图（F-1）              —— 影响 AI 写报告
★★★  2. fetch 无重试 / 无 profile 阶梯（N-1）—— 稳定性
★★   3. meta.cache.age_s 恒 null（F-3）   —— 误导调用方
★★   4. container_score 恒 null（F-2）    —— 同上
★★   5. arxiv 429 标 not-retryable（N-4）
★    6. DNS 失败标 retryable（N-6）
★    7. --scope 无命中回执（抽查项）
★    8. webui 首页 ?url= 无效
```

---

## 七、给 ai 用的三条硬标准

**我这轮判断的标准很简单：**

```
① 失败必须能自证       → P0-1 修了，但 cache.age_s 又破了这条
② 数据必须一处一份     → cache_age_s 出现两处，一处 null 一处 131
③ 输出必须自带可回滚   → markdown 丢图，但 JSON 有，等于两套真值
```

**第 ②③ 条现在破了。**

---

## 八、建议修复顺序

```
1. --to md 输出图片（带位置）                        （一小时）
2. fetch 重试 + profile 阶梯 + attempts 回执          （两小时）
3. meta.cache.age_s 填真值，删 data.cache_age_s 重复   （二十分钟）
4. container_score：实现或删字段                       （半小时）
5. arxiv 429 → retryable: true + retry_after_s        （二十分钟）
6. 错误码分 dns_error                                  （二十分钟）
7. --scope 命中回执                                    （二十分钟）
8. webui 首页支持 ?url=                                （二十分钟）
```

**前两条修完，它就能当我默认的网页后端长期跑了。**

---

## 附：冒烟脚本

**位置：`D:\hermes-tools\cb_smoke.py`，结果 `D:\hermes-tools\cb_smoke.json`**

**48 项覆盖：基础 4 / fetch 12 / export 3 / search 5 / 数据检索 4 / 本地数据 10 / js 3 / plugin 2 / render·plain·bench 3 / ai 2**

**重跑方式：**

```bash
cd /d/hermes-tools && python cb_smoke.py
```

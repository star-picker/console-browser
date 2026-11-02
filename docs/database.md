# 数据库设计 · Database design

cb 把一切都存在**本机一个 sqlite 文件**里：页面、缓存、历史、日志、设置。
Everything lives in one local sqlite file: pages, cache, history, logs, settings.

## 文件在哪 · Where the file is

| 方式 / How | 值 / Value |
|---|---|
| 默认 / default | `%LOCALAPPDATA%\console-browser\cb.db` |
| 环境变量 / env | `CB_DB=D:\path\cb.db` |
| 命令行 / flag | `cb --db D:\path\cb.db …` |
| 交互模式 / shell | 启动时的 `--db` 会一直沿用 / the startup `--db` is reused |

WAL 模式（`cb.db-wal`、`cb.db-shm` 是正常伴生文件）。WAL mode is on; the `-wal` / `-shm` siblings are normal.

## 表 · Tables（schema v1）

`PRAGMA user_version` = `1`。迁移靠版本号推进，重复启动幂等。
Migrations are version-gated by `PRAGMA user_version`; re-running is idempotent.

### pages — 抓过的页面 / fetched pages

| 列 Column | 用途 Purpose |
|---|---|
| `id` | 主键 PK |
| `url` UNIQUE | 请求 URL（唯一键，重复抓取走 upsert） |
| `url_hash` | blake3(url)，给后续索引用 |
| `canonical` | 归一化后的最终 URL（去 fragment、默认端口小写化）/ normalized final URL |
| `site` | 主机名 host |
| `title` | 页面标题 |
| `status` | HTTP 状态码 |
| `content_type` | Content-Type |
| `fetched_at` | Unix 秒 unix seconds |
| `content_sha` | 正文 blake3，用于「同页二次读」判定 |
| `md` | 提取后的 markdown（BLOB） |
| `raw` | 原始 HTML（BLOB） |
| `etag` / `last_modified` | 校验器 validators |
| `bytes` / `tokens` | 体积与 token 估算 |
| `lang` | 语言（预留 reserved） |

### cache_entries — HTTP 缓存 / HTTP cache

| 列 Column | 用途 Purpose |
|---|---|
| `key` PK | `blake3(method, 归一化URL, profile, js_flag)` |
| `method` / `url` | 请求标识 |
| `status` / `headers` | 响应状态与头（JSON） |
| `body` | 响应体（BLOB，按字符集解码前保存） |
| `expires_at` | 过期时间 = 抓取时刻 + `Cache-Control: max-age`（无则默认 7 天；`no-store`/`no-cache` 则不缓存） |
| `validators` | JSON：`etag`、`last_modified`、`final_url`（重定向目标） |
| `created_at` | 写入时刻，用于算 cache age |

**键设计**：换 TLS profile 或开关 JS 都会得到不同的键 —— 不同指纹拿到的页面本来就可能不同。
Different TLS profile or JS flag means a different key, because the server may answer differently.

### history — 浏览历史 / browsing history

| 列 Column | 用途 Purpose |
|---|---|
| `ts` / `url` / `title` | 发生了什么 |
| `kind` | `fetch` / `export` / `search` / `agent`（后三种 M2 起） |
| `session_id` | 会话（交互模式/agent 预留） |
| `duration_ms` / `ok` | 耗时与成败 |

### logs — 台账 / operational log rows

结构化事件行（目前由 `cb db prune` 写入清理记录；文件日志在 M2）。
Structured rows; currently written by `cb db prune`; file logging lands in M2.

### settings — 键值 / key-value

`lang`（界面语言）、将来放默认 profile、预算等。

## 保留期与清理 / Retention

| 数据 | 保留 | 命令 |
|---|---|---|
| 缓存 cache | 到 `expires_at`（默认 7 天） | `cb db prune` |
| 历史 history | 90 天 | `cb db prune` |
| 日志 logs | 14 天 | `cb db prune` |
| 页面 pages | 不自动删（是资产） | 手动 / manual |

## 怎么看 / How to inspect

```powershell
cb db stats                      # 行数 row counts
cb db schema                     # 表 + 列 + 行数
cb --json db schema              # 机器可读 machine readable
cb history --limit 10
sqlite3 "$env:LOCALAPPDATA\console-browser\cb.db" ".tables"
sqlite3 "$env:LOCALAPPDATA\console-browser\cb.db" "select url, title from history order by ts desc limit 5;"
```

## 为什么这样设计 / Why these choices

- **一个文件**：可备份、可删、可拷走；不上服务，不引网络数据库。
- **页面与缓存分表**：`pages` 是「我读过什么」（长期资产），`cache_entries` 是「我还能不能不再请求」（会过期）。
- **content_sha + markdown 双存**：二次读能只回差量，省 token；也方便以后做 FTS5 全文检索。
- **凭证边界**：cassette 录制会剔除 `Set-Cookie` / `Authorization`；库里不存 API key（key 只从环境变量/配置文件读）。
- **cookie 加密**：`cookies.value` 存的是 `SecretBox` 封装后的字节。Windows 默认用 **DPAPI**（同用户可解、跨用户不可读），其它平台退回 `plain:` 标记的明文并在文档里明说；旧数据两种格式都能读。

### llm_calls — 模型调用账本 / model call ledger（v2）

每次调用一行：`ts`、`day`（天序号）、`provider`、`model`、`purpose`（ask/agent/challenge…）、`prompt_tokens`、`completion_tokens`、`latency_ms`、`ok`、`error`。失败的调用也记，只是不计入用量。

### ai_budget — 按天累计用量 / daily token usage（v2）

`day` 主键 + `tokens_used`。`record_llm_call` 在同一事务里累加；`CB_AI_DAILY_BUDGET` 拍上限后，超了直接拒绝（退出码 9），不烧钱。

### challenge_events — 挑战记录 / anti-bot events（v2）

每次 403 尝试一行：`domain`、`kind`、`strategy`（如 `rotate-profile:firefox`）、`ok`、`detail`。`cb challenge stats` 按域名×策略汇总胜负。

## 后续（M3 计划表）

- `page_revisions`：同 URL 多次抓取的 diff 快照（差量读）。
- `site_strategy`：把 challenge_events 的学习结果物化成每域首选策略。
- `sessions` / `session_events`：交互与 agent 会话回放。
- `page_fts`（FTS5）：站点内/历史全文检索。
- `plugins`：插件登记与启用状态。
- `page_fts`（FTS5）：站点内/历史全文检索。
- `plugins`：插件登记与启用状态。
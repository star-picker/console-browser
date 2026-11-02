# 全屏 TUI 浏览器 · 交互说明 / Interactive TUI guide

`cb`（不带参数，或双击 exe）在真终端里进入全屏浏览器；管道/CI 里自动回 REPL。
Run `cb` in a real terminal (or double-click the exe) to get the full-screen browser; when piped, it falls back to the REPL.

## 屏幕结构 / Screen layout

```
▸ 地址栏 / omnibox        ← 默认聚焦：打网址或搜索词，回车即加载
[1 页面标题 │ 2 …]        ← 标签条
                          正文（reader 渲染：标题/段落/列表/表格/代码/引用）
BROWSE 42% · 120行 · 350tok  → https://聚焦链接    ← 状态栏
```

**只在同一屏内刷新**（alternate screen + 整屏重绘），退出或崩溃都会恢复原终端；日志不写屏幕（走文件）。
**Everything redraws in place** (alternate screen); the terminal is restored on exit or panic, and logs never pollute the screen.

## 地址栏规则 / Omnibox rules

| 输入 | 结果 |
|---|---|
| `example.com` / `https://…` / `localhost:8080` | 直接导航（域名补 https，localhost 补 http） |
| `rust http client` | 默认引擎搜索（结果页渲染成普通文档，Enter 打开聚焦结果） |
| `!gh ripgrep` / `!arxiv diffusion` / `!bing …` | 指定引擎直搜 |
| `?` / `/` | 直接呼出帮助浮层（键位表） |
| `cb:help` `cb:history` `cb:links` `cb:palette` `cb:trace` `cb:ai` `cb:reload` `cb:back` `cb:forward` `cb:top` `cb:bottom` `cb:quit` `cb:export-md` `cb:export-html` `cb:lang` | 界面内部动作（地址栏也能触发，便于发现） |
| `cb:<其它>` | 当作原 CLI 参数执行 |
| `:fetch https://a/ --to md` | 直接跑原 CLI 参数（子进程），输出进状态栏/日志 |

## 键位 / Keymap

| 键 | 动作 |
|---|---|
| `l` / `Ctrl-L` | 聚焦地址栏 |
| `Enter` | 打开聚焦链接（同标签） / `Ctrl-Enter` 新标签打开 |
| `Tab` / `Shift-Tab` | 链接焦点循环（状态栏显示目标 URL） |
| `j k` `Space` `PgDn` `PgUp` `g` `G` | 滚动 / 翻页 / 顶 / 底 |
| `b` `f` `r` `Esc` | 后退 / 前进 / 重载 / 取消加载 |
| `/` `n` `N` | 页内查找 / 下一个 / 上一个 |
| `L` | 链接列表浮层（序号 + URL，Enter 打开） |
| `Ctrl-H` / `Ctrl-P` | 历史 / 命令面板 |
| `:` | 命令行（原 CLI 参数） |
| `Ctrl-A` / `e` / `c` | AI 面板 / 人工接力 / 粘贴 Cookie |
| `v` `Ctrl-R` | 原始文本 / markdown 视图 |
| `Ctrl-E` `Ctrl-S` `y` | 导出 md / 导出单文件 html / 复制 URL |
| `Ctrl-T` `Ctrl-W` `Ctrl-Tab` `Alt-1..9` | 新标签 / 关标签 / 切换 / 直选 |
| `?` | 帮助浮层 |
| `q` / `Ctrl-Q` | 退出 / 强制退出 |

## 加载 / 错误 / 需要人 / Loading, errors, human-in-the-loop

- 提交即显示 `loading…`（旧页面保留到新内容到达，不闪白）；`Esc` 取消（迟到的结果按加载代数丢弃）。
- 失败渲染成错误页：状态码 + 人话原因；`r` 重试、`b` 返回。
- 需要登录 / 人机验证 → 「需要你」页：`e` 打开人工接力窗口，`c` 粘贴 Cookie。
- **人工接力**（T11 实现中）：用系统里已有的 Edge/Chrome 以临时 profile + 远程调试端口打开目标页，人登录/过滑块后，cb 通过 CDP 收集 cookie 存进本地 jar 并自动重载。

## 会话 / Session

退出时把标签页（URL、标题、滚动、导航栈）写进本地 sqlite，下次启动恢复；`cb tui --no-restore` 跳过恢复。
Tabs are persisted to sqlite on exit and restored on start; use `--no-restore` to skip.

## 非 TUI 入口 / Non-TUI entry points

```powershell
cb shell                  # 原来的逐行 REPL（命令式）
cb fetch <url> --to text  # 纯文本输出（管道友好）
cb mcp serve              # MCP 服务端（给 agent）
```
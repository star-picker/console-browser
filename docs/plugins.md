# 插件使用指南（从零开始）

> 目标读者：想给某类站点定制正文范围、又不想改 cb 源码的人。
> 插件目前有两种生效方式：**声明式选择器**（零代码）和 **JS 算选择器**（安全受限）。

## 1. 插件放在哪

```
%APPDATA%\console-browser\plugins\<插件名>\plugin.json
```

- `cb plugin dir` 直接打印路径
- 测试/隔离时可用环境变量覆盖：`$env:CB_PLUGIN_DIR = "D:\my-plugins"`

## 2. 最小插件（声明式，推荐先从这个开始）

`plugins/wechat/plugin.json`：

```json
{
  "name": "wechat",
  "version": "1.0.0",
  "description": "公众号正文容器",
  "permissions": ["extract"],
  "match": ["mp.weixin.qq.com"],
  "hooks": { "extract": { "selectors": ["#js_content"] } }
}
```

之后抓公众号文章，正文范围就只取 `#js_content`：

```powershell
cb fetch "https://mp.weixin.qq.com/s/xxxx" --to md
```

顺序：**插件 JS 钩子 → 插件的 `selectors` → 用户的 `--scope`**。用户显式给了 `--scope` 就不看插件。

## 3. 加 JS 钩子（可选）

当选择器要根据页面结构变化时，用脚本算：

`plugins/wechat/sel.js`：

```js
document.querySelectorAll('#js_content').length ? '#js_content' : 'body'
```

`plugin.json` 里加一行 `"script": "sel.js"`。脚本的返回值就是 CSS 选择器；返回空串会回退到 `selectors`。

### 脚本能做什么 / 不能做什么

| 能力 | 状态 |
|---|---|
| `document.title` / `body.innerText` | ✅ 读 |
| `document.querySelectorAll('#id/.class/tag')` | ✅ 读（只读快照，最多 400 个元素） |
| `location.href/host`、`navigator`、`localStorage` | ✅ 桩实现 |
| 发网络请求 | ❌ 没有 fetch 桥（不会偷偷出网） |
| 改 DOM | ❌ 只读快照（完整可写 DOM 尚未实现） |
| 死循环 | ⛔ 2 秒中断 |

权限门槛：**必须有 `extract` 权限**，否则整个钩子不执行（fail closed）。

## 4. 常用命令

```powershell
cb plugin dir                                  # 插件目录
cb plugin list                                 # 名字 版本 [权限] 匹配域名 描述
cb plugin info wechat                          # 详情（含 match）
cb plugin add .\my-plugin --dir D:\my-plugins  # 校验+复制安装
cb --json plugin list                          # 机器可读
```

`cb plugin add` 会先校验清单（名字安全、版本必填、权限白名单），再整目录复制；同名覆盖。

## 5. 权限说明

| 权限 | 允许的事 |
|---|---|
| `extract` | 读 DOM 快照、参与正文范围选择 |
| `net` | 走 cb-net 发请求（尚未开放给插件） |
| `challenge` | 参与挑战处理（预留） |
| `command` | 注册 CLI 子命令（预留） |

未知权限会让整个插件被判坏（不会默默忽略）。

## 6. 常见问题

**Q：插件装了但没生效？**
1. `cb plugin list` 看 `match` 列是否有你的域名；
2. 确认 `permissions` 里有 `extract`；
3. 确认 `hooks.extract.selectors` 或 `script` 非空；
4. 如果你命令里带了 `--scope`，插件会被跳过（显式优先）。

**Q：JS 脚本写了但没跑？**
- 检查 `script` 指向的文件确实在插件目录里；
- 脚本抛错或超时（>2s）会静默回退到 `selectors`；
- 用 `cb js <url> --code "..."` 先单独验证脚本逻辑。

**Q：能改页面内容吗？**
- 暂时不能。当前是只读快照，只能决定「取哪块」。
- 完整可写 DOM（`innerHTML` 改写、回写 RenderDoc）在路线图里。

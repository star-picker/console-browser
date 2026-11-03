#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""cb 全功能冒烟测试 —— 输出结构化结果供报告使用"""
import subprocess, json, sys, os, time, shlex

sys.stdout.reconfigure(encoding='utf-8', errors='replace')
CB = r'D:\console-browser\target\debug\cb.exe'
OUT = []

def run(args, timeout=120, stdin_data=None):
    t0 = time.time()
    try:
        p = subprocess.run([CB] + args, capture_output=True, timeout=timeout,
                           input=stdin_data.encode() if stdin_data else None)
        ms = int((time.time() - t0) * 1000)
        out = p.stdout.decode('utf-8', 'replace')
        err = p.stderr.decode('utf-8', 'replace')
        return {'cmd': ' '.join(args), 'code': p.returncode, 'ms': ms,
                'out': out, 'err': err}
    except subprocess.TimeoutExpired:
        return {'cmd': ' '.join(args), 'code': -1,
                'ms': int((time.time()-t0)*1000), 'out': '', 'err': 'TIMEOUT'}
    except Exception as e:
        return {'cmd': ' '.join(args), 'code': -2, 'ms': 0, 'out': '', 'err': str(e)}


def j(r):
    """尝试把 stdout 当 JSON 解析"""
    try:
        return json.loads(r['out'])
    except Exception:
        return None


def rec(name, r, note=''):
    d = j(r)
    ok = r['code'] == 0
    if d is not None:
        ok = ok and (d.get('ok') is not False)
    OUT.append({'name': name, 'ok': ok, 'code': r['code'], 'ms': r['ms'],
                'note': note, 'json_ok': d is not None,
                'preview': (r['out'][:400] if r['out'] else r['err'][:400])})
    flag = '✓' if ok else '✗'
    print(f'{flag} {name:34} {r["code"]:3}  {r["ms"]:6}ms  {note}')


# ══════════ 基础 ══════════
print('═══ 基础 ═══')
rec('version', run(['--version']))
rec('help', run(['--help']))
rec('selftest --json', run(['selftest', '--json']))
rec('tools --json', run(['--json', 'tools']))

# ══════════ fetch 家族 ══════════
print('\n═══ fetch ═══')
URL = 'https://www.ruanyifeng.com/blog/2024/01/weekly-issue-287.html'
rec('fetch --to md', run(['fetch', URL, '--to', 'md']))
rec('fetch --to text', run(['fetch', URL, '--to', 'text']))
rec('fetch --to html', run(['fetch', URL, '--to', 'html']))
rec('fetch --json', run(['fetch', URL, '--json']))
rec('fetch --max-tokens 500', run(['fetch', URL, '--json', '--max-tokens', '500']))
rec('fetch --scope', run(['fetch', URL, '--json', '--scope', 'article']))
rec('fetch --fresh', run(['fetch', URL, '--json', '--fresh']))
rec('fetch --profile firefox', run(['fetch', URL, '--json', '--profile', 'firefox']))
rec('fetch --profile safari', run(['fetch', URL, '--json', '--profile', 'safari']))
rec('fetch --out 文件', run(['fetch', URL, '--to', 'md', '--out', r'D:\hermes-tools\t1.md']))
rec('fetch 404页', run(['fetch', 'https://www.ruanyifeng.com/nope-404.html', '--json']))
rec('fetch 坏域名', run(['fetch', 'https://this-domain-does-not-exist-xyz.com', '--json']))

# ══════════ export ══════════
print('\n═══ export ═══')
rec('export --to md', run(['export', URL, '--to', 'md']))
rec('export --to html', run(['export', URL, '--to', 'html', '--out', r'D:\hermes-tools\t1.html']))
rec('export --json', run(['export', URL, '--json', '--out', r'D:\hermes-tools\t1b.html', '--to', 'html']))

# ══════════ search ══════════
print('\n═══ search ═══')
for eng in ['auto', 'gh', 'bing', 'ddg', 'arxiv']:
    rec(f'search --engine {eng}', run(['search', 'rust async runtime', '--engine', eng,
                                       '--limit', '3', '--json'], timeout=90))

# ══════════ history / revisions / site ══════════
print('\n═══ 数据检索 ═══')
rec('history --limit 5', run(['history', '--limit', '5', '--json']))
rec('history --query', run(['history', '--query', 'ruanyifeng', '--json']))
rec('revisions', run(['revisions', URL, '--json'], timeout=90))
rec('site list', run(['site', 'list', '--json']))

# ══════════ 本地数据 ══════════
print('\n═══ 本地数据 ═══')
rec('db schema', run(['db', 'schema', '--json']))
rec('db stats', run(['db', 'stats', '--json']))
rec('bookmark add', run(['bookmark', 'add', URL, '--title', '测试书签', '--json']))
rec('bookmark list', run(['bookmark', 'list', '--json']))
rec('bookmark rm', run(['bookmark', 'rm', URL, '--json']))
rec('cookies list', run(['cookies', 'list', '--json']))
rec('log dir', run(['log', 'dir', '--json']))
rec('log stats', run(['log', 'stats', '--json']))
rec('log tail', run(['log', 'tail', '--limit', '5', '--json']))
rec('challenge stats', run(['challenge', 'stats', '--json']))

# ══════════ js ══════════
print('\n═══ js ═══')
rec('js --code 简单', run(['js', '--code', '1+1', '--json']))
rec('js --code DOM', run(['js', '--code', 'document.title', '--json', URL]))
rec('js --run-inline', run(['js', URL, '--run-inline', '--json'], timeout=90))

# ══════════ plugin ══════════
print('\n═══ plugin ══════════')
rec('plugin dir', run(['plugin', 'dir', '--json']))
rec('plugin list', run(['plugin', 'list', '--json']))

# ══════════ render / plain / bench ══════════
print('\n═══ render / plain / bench ═══')
rec('render', run(['render', URL, '--width', '100', '--height', '30', '--json'], timeout=90))
rec('plain', run(['plain', URL, '--json']))
rec('bench --n 2', run(['bench', URL, '--n', '2', '--json'], timeout=180))

# ══════════ ai ══════════
print('\n═══ ai ══════════')
rec('ai config', run(['ai', 'config', '--json']))
rec('ai usage', run(['ai', 'usage', '--json']))

# ══════════ 汇总 ══════════
print('\n' + '═'*70)
okc = sum(1 for o in OUT if o['ok'])
print(f'{"✓ PASS" if okc == len(OUT) else "结果"}  {okc}/{len(OUT)}')
print('═'*70)
print('\n失败项:')
for o in OUT:
    if not o['ok']:
        print(f"  ✗ {o['name']:32} code={o['code']} {o['preview'][:150]}")

json.dump(OUT, open(r'D:\hermes-tools\cb_smoke.json', 'w', encoding='utf-8'),
          ensure_ascii=False, indent=1)
print('\n已存 D:\\hermes-tools\\cb_smoke.json')

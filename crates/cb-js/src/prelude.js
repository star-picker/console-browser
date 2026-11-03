globalThis.document = { title: __TITLE__, body: { innerText: __TEXT__ } };
const __els = __ELS__;
document.querySelectorAll = (sel) => __els.filter((e) => {
  if (sel.startsWith('#')) return e.id === sel.slice(1);
  if (sel.startsWith('.')) return e.class.split(/\s+/).includes(sel.slice(1));
  return e.tag === sel.toLowerCase();
}).map((e) => ({ ...e, textContent: e.text }));
globalThis.location = { href: __URL__, host: (__URL__ || '').replace(/^\w+:\/\//, '').split('/')[0] };
const __store = new Map();
globalThis.localStorage = {
  getItem: (k) => (__store.has(k) ? __store.get(k) : null),
  setItem: (k, v) => { __store.set(String(k), String(v)); },
  removeItem: (k) => { __store.delete(k); },
  clear: () => { __store.clear(); },
  get length() { return __store.size; },
};
globalThis.navigator = { userAgent: 'cb-js/1 (console-browser)', language: __LANG__ };
globalThis.console = {
  log: (...a) => { globalThis.__cb_log.push(a.map(String).join(' ')); },
  warn: (...a) => { globalThis.__cb_log.push(a.map(String).join(' ')); },
  error: (...a) => { globalThis.__cb_log.push(a.map(String).join(' ')); },
};
globalThis.__cb_log = [];

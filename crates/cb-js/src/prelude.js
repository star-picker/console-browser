globalThis.document = { title: __TITLE__, body: { innerText: __TEXT__ } };
const __els = __ELS__;
document.querySelectorAll = (sel) => __els.filter((e) => {
  if (sel.startsWith('#')) return e.id === sel.slice(1);
  if (sel.startsWith('.')) return e.class.split(/\s+/).includes(sel.slice(1));
  return e.tag === sel.toLowerCase();
}).map((e) => ({ ...e, textContent: e.text }));

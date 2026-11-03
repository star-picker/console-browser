// DOM/BOM 桩：真实父子树 + JS 本地可变属性/事件；不是浏览器布局。
// 事件循环由 Rust 侧驱动：__cb_fire_due / __cb_next_wait。

function __cb_classes(el) {
  return String(el.getAttribute('class') || '').split(/\s+/).filter(function (c) { return c.length > 0; });
}

function __cb_make_el(spec, parent) {
  const el = {
    nodeType: 1,
    tag: String(spec.tag || 'div').toLowerCase(),
    attrs: Object.assign({}, spec.attrs || {}),
    children: [],
    parentElement: parent || null,
    parentNode: parent || null,
    __directText: spec.text || '',
    __listeners: {},
    get id() { return this.getAttribute('id') || ''; },
    set id(v) { this.setAttribute('id', v); },
    get className() { return this.getAttribute('class') || ''; },
    set className(v) { this.setAttribute('class', v); },
    get textContent() {
      let out = this.__directText || '';
      for (const child of this.children) out += child.textContent || '';
      return out;
    },
    set textContent(v) {
      this.children = [];
      this.__directText = String(v);
    },
    get innerText() {
      let out = this.__directText || '';
      for (const child of this.children) {
        if (child.nodeType === 1 && (child.tag === 'script' || child.tag === 'style' ||
            child.tag === 'template' || child.tag === 'noscript')) continue;
        out += ' ' + (child.innerText || child.textContent || '');
      }
      return out.replace(/\s+/g, ' ').trim();
    },
    set innerText(v) { this.textContent = v; },
    get firstChild() { return this.children.length ? this.children[0] : null; },
    get lastChild() { return this.children.length ? this.children[this.children.length - 1] : null; },
    get firstElementChild() { return this.__childrenByType(1)[0] || null; },
    get lastElementChild() {
      const kids = this.__childrenByType(1);
      return kids.length ? kids[kids.length - 1] : null;
    },
    __childrenByType(type) {
      return this.children.filter(function (c) { return c.nodeType === type; });
    },
    getAttribute(name) {
      name = String(name).toLowerCase();
      return Object.prototype.hasOwnProperty.call(this.attrs, name) ? this.attrs[name] : null;
    },
    setAttribute(name, value) {
      this.attrs[String(name).toLowerCase()] = String(value);
    },
    hasAttribute(name) {
      return Object.prototype.hasOwnProperty.call(this.attrs, String(name).toLowerCase());
    },
    removeAttribute(name) { delete this.attrs[String(name).toLowerCase()]; },
    get classList() {
      const el = this;
      return {
        contains: function (name) { return __cb_classes(el).indexOf(String(name)) >= 0; },
        add: function () {
          const set = __cb_classes(el);
          for (const name of arguments) if (set.indexOf(String(name)) < 0) set.push(String(name));
          el.setAttribute('class', set.join(' '));
        },
        remove: function () {
          const drop = Array.prototype.map.call(arguments, String);
          el.setAttribute('class', __cb_classes(el).filter(function (c) { return drop.indexOf(c) < 0; }).join(' '));
        },
        toggle: function (name) {
          name = String(name);
          if (this.contains(name)) { this.remove(name); return false; }
          this.add(name);
          return true;
        },
        get length() { return __cb_classes(el).length; },
      };
    },
    addEventListener(type, fn) {
      type = String(type);
      if (!this.__listeners[type]) this.__listeners[type] = [];
      this.__listeners[type].push(fn);
    },
    removeEventListener(type, fn) {
      const list = this.__listeners[String(type)] || [];
      const index = list.indexOf(fn);
      if (index >= 0) list.splice(index, 1);
    },
    dispatchEvent(ev) {
      if (ev && !ev.target) ev.target = this;
      const list = (this.__listeners[ev && ev.type] || []).slice();
      for (const fn of list) fn.call(this, ev);
      return true;
    },
    click() { this.dispatchEvent({ type: 'click', target: this }); },
    appendChild(child) {
      if (child.parentElement && child.parentElement.removeChild) child.parentElement.removeChild(child);
      child.parentElement = this;
      child.parentNode = this;
      this.children.push(child);
      return child;
    },
    insertBefore(child, reference) {
      if (child.parentElement && child.parentElement.removeChild) child.parentElement.removeChild(child);
      const index = reference ? this.children.indexOf(reference) : -1;
      if (index < 0) return this.appendChild(child);
      child.parentElement = this;
      child.parentNode = this;
      this.children.splice(index, 0, child);
      return child;
    },
    removeChild(child) {
      const index = this.children.indexOf(child);
      if (index >= 0) {
        this.children.splice(index, 1);
        child.parentElement = null;
        child.parentNode = null;
      }
      return child;
    },
    querySelector(selector) {
      return __cb_query(this, selector, false)[0] || null;
    },
    querySelectorAll(selector) {
      return __cb_query(this, selector, false);
    },
    getElementsByTagName(tag) {
      tag = String(tag).toLowerCase();
      const out = __cb_query(this, tag === '*' ? '*' : tag, false);
      return out;
    },
  };
  return el;
}

function __cb_build(spec, parent) {
  const el = __cb_make_el(spec, parent);
  for (const childSpec of (spec.children || [])) {
    el.children.push(__cb_build(childSpec, el));
  }
  return el;
}

const __cb_root = __cb_build(__TREE__, null);

function __cb_walk(root, out) {
  if (root.nodeType === 1) out.push(root);
  for (const child of root.children || []) __cb_walk(child, out);
  return out;
}

function __cb_find_tag(root, tag) {
  for (const child of root.children || []) {
    if (child.nodeType === 1 && child.tag === tag) return child;
    const nested = __cb_find_tag(child, tag);
    if (nested) return nested;
  }
  return null;
}

const __cb_document_element = __cb_root;
const __cb_head = __cb_find_tag(__cb_root, 'head') || __cb_make_el({ tag: 'head' }, __cb_root);
const __cb_body = __cb_find_tag(__cb_root, 'body') || (function () {
  const body = __cb_make_el({ tag: 'body' }, __cb_root);
  __cb_root.children.push(body);
  return body;
})();

// ---- 选择器引擎：逗号组 + 后代/子代 + 复合选择器 ----
function __cb_parse_steps(selector) {
  const steps = [];
  let current = '';
  let comb = ' ';
  let i = 0;
  while (i < selector.length) {
    const ch = selector.charAt(i);
    if (ch === '>' || ch === ' ' || ch === '\t' || ch === '\n') {
      if (current.length) {
        steps.push({ comb: comb, part: current });
        current = '';
      }
      let sawChild = false;
      while (i < selector.length) {
        const sep = selector.charAt(i);
        if (sep === '>') { sawChild = true; i++; continue; }
        if (sep === ' ' || sep === '\t' || sep === '\n') { i++; continue; }
        break;
      }
      comb = sawChild ? '>' : ' ';
      continue;
    }
    current += ch;
    i++;
  }
  if (current.length) steps.push({ comb: comb, part: current });
  return steps;
}

function __cb_match_simple(el, part) {
  if (!el || el.nodeType !== 1) return false;
  let rest = part.trim();
  const tagMatch = rest.match(/^([a-zA-Z][\w-]*|\*)/);
  if (tagMatch) {
    if (tagMatch[1] !== '*' && el.tag !== tagMatch[1].toLowerCase()) return false;
    rest = rest.slice(tagMatch[1].length);
  }
  while (rest.length) {
    if (rest.charAt(0) === '#') {
      const m = rest.match(/^#([\w-]+)/);
      if (!m || el.id !== m[1]) return false;
      rest = rest.slice(m[0].length);
    } else if (rest.charAt(0) === '.') {
      const m = rest.match(/^\.([\w-]+)/);
      if (!m || el.classList.contains(m[1]) === false) return false;
      rest = rest.slice(m[0].length);
    } else if (rest.charAt(0) === '[') {
      const m = rest.match(/^\[([^\]=]+)(?:=(?:"([^"]*)"|'([^']*)'|([^\]]*)))?\]/);
      if (!m) return false;
      const name = m[1].trim().toLowerCase();
      const value = m[2] !== undefined ? m[2] : (m[3] !== undefined ? m[3] : m[4]);
      if (!el.hasAttribute(name)) return false;
      if (value !== undefined && el.getAttribute(name) !== value) return false;
      rest = rest.slice(m[0].length);
    } else if (rest.indexOf(':first-child') === 0) {
      if (!el.parentElement || el.parentElement.children[0] !== el) return false;
      rest = rest.slice(':first-child'.length);
    } else if (rest.indexOf(':last-child') === 0) {
      const kids = el.parentElement ? el.parentElement.children : [];
      if (!kids.length || kids[kids.length - 1] !== el) return false;
      rest = rest.slice(':last-child'.length);
    } else if (rest.indexOf(':nth-child(') === 0) {
      const m = rest.match(/^:nth-child\((\d+)\)/);
      if (!m) return false;
      const index = Number(m[1]) - 1;
      if (!el.parentElement || el.parentElement.children[index] !== el) return false;
      rest = rest.slice(m[0].length);
    } else {
      return false;
    }
  }
  return true;
}

function __cb_match_steps(el, steps, index) {
  if (!__cb_match_simple(el, steps[index].part)) return false;
  if (index === 0) return true;
  if (steps[index].comb === '>') return __cb_match_steps(el.parentElement, steps, index - 1);
  let parent = el.parentElement;
  while (parent) {
    if (__cb_match_steps(parent, steps, index - 1)) return true;
    parent = parent.parentElement;
  }
  return false;
}

function __cb_matches(el, selector) {
  for (const group of String(selector).split(',')) {
    const steps = __cb_parse_steps(group.trim());
    if (!steps.length) continue;
    if (__cb_match_steps(el, steps, steps.length - 1)) return true;
  }
  return false;
}

function __cb_query(scope, selector, includeSelf) {
  const all = [];
  if (includeSelf) __cb_walk(scope, all);
  else for (const child of scope.children || []) __cb_walk(child, all);
  return all.filter(function (el) { return __cb_matches(el, selector); });
}

const __cb_doc_listeners = {};

globalThis.window = globalThis;
globalThis.document = {
  nodeType: 9,
  documentElement: __cb_document_element,
  head: __cb_head,
  body: __cb_body,
  get title() {
    const titleEl = __cb_find_tag(__cb_root, 'title');
    return titleEl ? titleEl.textContent.trim() : __TITLE__;
  },
  set title(value) {
    const titleEl = __cb_find_tag(__cb_root, 'title');
    if (titleEl) titleEl.textContent = String(value);
  },
  querySelector: function (selector) { return __cb_query(__cb_document_element, selector, true)[0] || null; },
  querySelectorAll: function (selector) { return __cb_query(__cb_document_element, selector, true); },
  getElementById: function (id) {
    id = String(id);
    return __cb_query(__cb_document_element, '#' + id, true)[0] || null;
  },
  getElementsByTagName: function (tag) {
    return __cb_query(__cb_document_element, tag === '*' ? '*' : String(tag), true);
  },
  createElement: function (tag) { return __cb_make_el({ tag: tag }, null); },
  createTextNode: function (text) {
    return {
      nodeType: 3,
      textContent: String(text),
      parentElement: null,
      parentNode: null,
      __directText: String(text),
    };
  },
  addEventListener: function (type, fn) {
    type = String(type);
    if (!__cb_doc_listeners[type]) __cb_doc_listeners[type] = [];
    __cb_doc_listeners[type].push(fn);
  },
  removeEventListener: function (type, fn) {
    const list = __cb_doc_listeners[String(type)] || [];
    const index = list.indexOf(fn);
    if (index >= 0) list.splice(index, 1);
  },
  dispatchEvent: function (ev) {
    const list = (__cb_doc_listeners[ev && ev.type] || []).slice();
    for (const fn of list) fn.call(globalThis.document, ev);
    return true;
  },
  readyState: 'complete',
};

globalThis.Node = { ELEMENT_NODE: 1, TEXT_NODE: 3 };
globalThis.location = {
  href: __URL__,
  host: (__URL__ || '').replace(/^\w+:\/\//, '').split('/')[0],
};
const __cb_store = new Map();
globalThis.localStorage = {
  getItem: function (k) { return __cb_store.has(k) ? __cb_store.get(k) : null; },
  setItem: function (k, v) { __cb_store.set(String(k), String(v)); },
  removeItem: function (k) { __cb_store.delete(String(k)); },
  clear: function () { __cb_store.clear(); },
  get length() { return __cb_store.size; },
};
globalThis.navigator = { userAgent: 'cb-js/1 (console-browser)', language: __LANG__ };
globalThis.__cb_log = [];
globalThis.console = {
  log: function () { __cb_log.push(Array.prototype.map.call(arguments, String).join(' ')); },
  warn: function () { __cb_log.push(Array.prototype.map.call(arguments, String).join(' ')); },
  error: function () { __cb_log.push(Array.prototype.map.call(arguments, String).join(' ')); },
};

// ---- 事件循环：定时器 + 微任务 ----
globalThis.__cb_timers = new Map();
globalThis.__cb_timer_seq = 0;
globalThis.setTimeout = function (fn, ms) {
  const id = ++__cb_timer_seq;
  __cb_timers.set(id, { fn: fn, at: Date.now() + (Number(ms) || 0), interval: null });
  return id;
};
globalThis.setInterval = function (fn, ms) {
  const id = ++__cb_timer_seq;
  const every = Math.max(1, Number(ms) || 1);
  __cb_timers.set(id, { fn: fn, at: Date.now() + every, interval: every });
  return id;
};
globalThis.clearTimeout = function (id) { __cb_timers.delete(Number(id)); };
globalThis.clearInterval = globalThis.clearTimeout;
globalThis.queueMicrotask = function (fn) { Promise.resolve().then(fn); };
globalThis.__cb_fire_due = function () {
  const now = Date.now();
  const due = [];
  for (const pair of __cb_timers) {
    if (pair[1].at <= now) due.push(pair[0]);
  }
  due.sort(function (a, b) { return a - b; });
  let fired = 0;
  for (const id of due) {
    const timer = __cb_timers.get(id);
    if (!timer) continue;
    try {
      timer.fn();
      fired++;
    } catch (e) {
      __cb_log.push('timer error: ' + e);
    }
    if (timer.interval) timer.at = Date.now() + timer.interval;
    else __cb_timers.delete(id);
  }
  return fired;
};
globalThis.__cb_next_wait = function () {
  let wait = -1;
  const now = Date.now();
  for (const timer of __cb_timers.values()) {
    const delta = Math.max(0, timer.at - now);
    if (wait < 0 || delta < wait) wait = delta;
  }
  return wait;
};

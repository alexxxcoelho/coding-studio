// Coding Studio web UI. Vanilla ES module, no build step, no network beyond
// the local API. Every write goes through the API, which records it in the
// audit trail; the UI never decides a protocol rule on its own.

const view = document.getElementById('view');
const enc = encodeURIComponent;

// ------------------------------------------------------------------ helpers

function h(tag, attrs, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === null || v === undefined || v === false) continue;
    if (k === 'class') el.className = v;
    else if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'value') el.value = v;
    else if (k === 'checked') el.checked = !!v;
    else if (k === 'disabled') el.disabled = !!v;
    else if (k === 'html') throw new Error('raw html is not allowed');
    else el.setAttribute(k, v === true ? '' : v);
  }
  append(el, kids);
  return el;
}

function append(el, kids) {
  for (const kid of kids.flat(Infinity)) {
    if (kid === null || kid === undefined || kid === false) continue;
    el.append(kid instanceof Node ? kid : document.createTextNode(String(kid)));
  }
  return el;
}

/** Appends children, skipping null, undefined and false (native append would print them). */
function put(el, ...kids) { return append(el, kids); }

function clear(el) { while (el.firstChild) el.firstChild.remove(); return el; }

async function api(method, path, body) {
  const opts = { method, headers: {} };
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json';
    opts.body = JSON.stringify(body);
  }
  const res = await fetch(path, opts);
  let data = null;
  try { data = await res.json(); } catch { data = null; }
  if (!res.ok) {
    const err = new Error((data && data.error) || `${res.status} ${res.statusText}`);
    err.kind = data && data.kind;
    err.issues = data && data.issues;
    err.status = res.status;
    throw err;
  }
  return data;
}

function toast(message, kind = '') {
  const box = document.getElementById('toasts');
  const t = h('div', { class: `toast ${kind}`, role: kind === 'error' ? 'alert' : 'status' }, message);
  put(box, t);
  setTimeout(() => t.remove(), kind === 'error' ? 9000 : 3500);
}

function fail(err) {
  const label = { guard: 'Protocol lock.', validation: 'Not accepted.', not_found: 'Not found.' }[err.kind] || 'Error.';
  toast(h('span', {}, h('span', { class: 'kind' }, label), err.message), 'error');
}

/** Runs an async action with a busy button, reporting errors as toasts. */
async function busy(button, label, fn) {
  const before = button.textContent;
  button.disabled = true;
  button.textContent = label;
  try { return await fn(); }
  catch (e) { fail(e); return undefined; }
  finally { button.disabled = false; button.textContent = before; }
}

function fmtTime(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
}

function fmtSize(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} kB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

function pct(a, b) { return b ? `${Math.round((100 * a) / b)} %` : ''; }

function nowLocalInput() {
  const d = new Date();
  d.setMinutes(d.getMinutes() - d.getTimezoneOffset());
  return d.toISOString().slice(0, 16);
}

function emptyState(title, text, action) {
  return h('div', { class: 'panel empty' }, h('strong', {}, title), h('p', {}, text), action || null);
}

// ----------------------------------------------------------------- codebook

let BOOK = null;

async function loadBook(force = false) {
  if (!BOOK || force) BOOK = await api('GET', '/api/codebook');
  return BOOK;
}

function codeInfo(code) {
  return BOOK && BOOK.codes.find((c) => c.code === code);
}

function chip(code, extra = '') {
  if (code === 'NA') return h('span', { class: `code na ${extra}`, title: 'No code applies' }, 'NA');
  const info = codeInfo(code);
  const fam = info ? info.family : 'CONTEXT';
  return h('span', { class: `code fam-${fam} ${extra}`, title: info ? `${info.label}: ${info.definition}` : 'Not in the codebook' }, code);
}

function chips(codes, markOnly) {
  const list = [...(codes || [])].sort();
  if (!list.length) return h('span', { class: 'muted small' }, 'none');
  return h('span', { class: 'codes' }, list.map((c) => chip(c, markOnly && markOnly(c) ? 'only' : '')));
}

const MODIFIERS = ['ENABLER', 'BARRIER', 'CONTESTED', 'ABSENT', 'CONTEXT-DEPENDENT', 'CHANGE-OVER-TIME', 'INTERACTION', 'NECESSARY-NOT-SUFFICIENT'];
const EV_LEVELS = ['EV0', 'EV1', 'EV2', 'EV3', 'EV4'];
const PROVENANCES = ['SPONTANEOUS', 'PROBED', 'DOCUMENT-DERIVED', 'RESEARCHER-INFERRED'];
const TRANSCRIPT_KINDS = new Set(['vtt', 'timestamped_text', 'cleaned_markdown', 'budgie_markdown', 'written_response']);
const KIND_LABELS = {
  vtt: 'Platform transcript (VTT)', timestamped_text: 'Raw ASR text', cleaned_markdown: 'Cleaned transcript',
  budgie_markdown: 'Budgie Echo export', written_response: 'Written response', summary: 'Summary',
  record_sheet: 'Record sheet', toe_coding: 'Earlier TOE coding', other: 'Other',
};

/** Code picker: search, grouped list, keyboard (arrows, Enter). */
function codePicker({ selected, onToggle, disabled }) {
  const search = h('input', { type: 'search', placeholder: 'Filter codes (press / to focus)', 'aria-label': 'Filter codes', disabled });
  const list = h('div', { class: 'picker', role: 'listbox', 'aria-multiselectable': 'true' });
  let options = [];
  let hl = -1;

  function render() {
    clear(list);
    options = [];
    const q = search.value.trim().toLowerCase();
    const groups = new Map();
    for (const c of BOOK.codes) {
      if (!c.active) continue;
      if (q && !(`${c.code} ${c.label} ${c.family}`.toLowerCase().includes(q))) continue;
      if (!groups.has(c.family)) groups.set(c.family, []);
      groups.get(c.family).push(c);
    }
    if (!groups.size) put(list, h('div', { class: 'muted small', style: 'padding:8px' }, 'No code matches.'));
    for (const [fam, codes] of groups) {
      put(list, h('div', { class: 'fam-head' }, fam));
      for (const c of codes) {
        const on = selected().has(c.code);
        const opt = h('button', {
          type: 'button', class: `opt ${on ? 'on' : ''}`, role: 'option', 'aria-selected': on ? 'true' : 'false',
          title: c.definition, disabled, onclick: () => { onToggle(c.code); render(); },
        }, h('span', { class: 'box' }), chip(c.code), h('span', { class: 'lbl' }, c.label));
        options.push({ el: opt, code: c.code });
        put(list, opt);
      }
    }
    if (hl >= options.length) hl = options.length - 1;
    if (hl >= 0 && options[hl]) options[hl].el.classList.add('hl');
  }

  search.addEventListener('input', () => { hl = 0; render(); });
  search.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); hl = Math.min(options.length - 1, hl + 1); render(); options[hl]?.el.scrollIntoView({ block: 'nearest' }); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); hl = Math.max(0, hl - 1); render(); options[hl]?.el.scrollIntoView({ block: 'nearest' }); }
    else if (e.key === 'Enter' && !(e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      const target = options[hl >= 0 ? hl : 0];
      if (target) { onToggle(target.code); render(); }
    } else if (e.key === 'Escape') { search.value = ''; hl = -1; render(); }
  });
  render();
  return { el: h('div', {}, search, list), search, render };
}

/** Dialog to add an inductive code. Resolves true when a code was added. */
function inductiveDialog() {
  return new Promise((resolve) => {
    const code = h('input', { type: 'text', placeholder: 'e.g. IND-TRUST-INTEGRATOR', required: true });
    const label = h('input', { type: 'text', required: true });
    const def = h('textarea', { rows: 3 });
    const add = h('button', { type: 'submit', class: 'primary' }, 'Add code');
    const dlg = h('dialog', {},
      h('form', { method: 'dialog', onsubmit: async (e) => {
        e.preventDefault();
        const ok = await busy(add, 'Adding...', async () => {
          await api('POST', '/api/codebook/codes', { code: code.value, label: label.value, definition: def.value });
          await loadBook(true);
          return true;
        });
        if (ok) { toast(`Inductive code ${code.value.trim()} added`, 'ok'); dlg.close(); resolve(true); dlg.remove(); }
      } },
      h('h2', {}, 'Add an inductive code'),
      h('p', { class: 'muted small' }, 'The code joins the INDUCTIVE family, is offered to the AI coder from the next prompt, and its creation is recorded in the audit trail.'),
      h('div', { class: 'field' }, h('label', {}, 'Code'), code),
      h('div', { class: 'field' }, h('label', {}, 'Label'), label),
      h('div', { class: 'field' }, h('label', {}, 'Definition'), def),
      h('div', { class: 'dialog-actions' },
        h('button', { type: 'button', onclick: () => { dlg.close(); dlg.remove(); resolve(false); } }, 'Cancel'),
        add)));
    document.body.append(dlg);
    dlg.showModal();
    code.focus();
  });
}

// ------------------------------------------------------------- chain status

async function refreshChain() {
  const box = document.getElementById('chain-status');
  try {
    const c = await api('GET', '/api/audit/verify');
    put(clear(box), c.ok
      ? h('a', { href: '#/audit', class: 'pill ok', title: 'Every audit event matches its SHA-256 chain' }, `Audit chain intact, ${c.n_events} events`)
      : h('a', { href: '#/audit', class: 'pill danger' }, `Audit chain broken at event ${c.first_bad_seq}`));
  } catch (e) {
    put(clear(box), h('span', { class: 'pill warn' }, 'Server unreachable'));
  }
}

// ------------------------------------------------------------------- router

let cleanup = null;

async function route() {
  if (cleanup) { cleanup(); cleanup = null; }
  const parts = location.hash.replace(/^#\/?/, '').split('/').map(decodeURIComponent);
  const page = parts[0] || 'library';
  document.querySelectorAll('[data-nav]').forEach((a) => a.classList.toggle('active', a.dataset.nav === (page === 'interview' ? 'library' : page)));
  put(clear(view), h('div', { class: 'loading' }, 'Loading...'));
  try {
    await loadBook();
    if (page === 'interview') await viewInterview(parts[1], parts[2] || 'sources');
    else if (page === 'codebook') await viewCodebook();
    else if (page === 'audit') await viewAudit();
    else await viewLibrary();
  } catch (e) {
    put(clear(view), emptyState('This page could not load.', e.message, h('button', { onclick: route }, 'Try again')));
  }
  refreshChain();
}

window.addEventListener('hashchange', route);

// ------------------------------------------------------------------ library

function nextStep(s) {
  if (!s.primary_source) return { label: 'Choose a transcript', tab: 'sources' };
  if (!s.n_units) return { label: 'Segment the transcript', tab: 'sources' };
  if (!s.human_complete) return { label: `Code ${s.n_units - s.n_human_coded} units`, tab: 'coding' };
  if (!s.n_ai_runs_complete) return { label: 'Run the AI coder', tab: 'ai' };
  if (s.n_arbitrated < s.n_divergences) return { label: `Arbitrate ${s.n_divergences - s.n_arbitrated}`, tab: 'arbitration' };
  return { label: 'Export the package', tab: 'audit' };
}

function ledger(s) {
  const step = (cls, text, title) => h('span', { class: `step ${cls}`, title }, text);
  const ai = s.n_ai_runs_complete ? 'done' : s.n_ai_runs ? 'part' : '';
  const arbDone = s.n_ai_runs_complete && s.n_arbitrated >= s.n_divergences;
  return h('div', { class: 'ledger' },
    step(s.primary_source ? 'done' : 'part', `${s.n_sources} files`, s.primary_source || 'No transcript chosen'),
    step(s.n_units ? 'done' : '', s.n_units ? `${s.n_units} units` : 'not segmented', 'Segmentation'),
    step(s.human_complete ? 'done' : s.n_human_coded ? 'part' : '', s.n_units ? `${s.n_human_coded}/${s.n_units} coded` : 'not coded', 'Your coding'),
    step(ai, `${s.n_ai_runs_complete} AI run${s.n_ai_runs_complete === 1 ? '' : 's'}`, `${s.n_ai_runs} run(s) recorded, ${s.n_ai_runs_complete} complete`),
    step(arbDone ? 'done' : s.n_divergences ? 'part' : '', s.n_ai_runs_complete ? `${s.n_arbitrated}/${s.n_divergences} arbitrated` : 'no arbitration', 'Divergences decided'),
  );
}

async function viewLibrary() {
  const [state, list] = await Promise.all([api('GET', '/api/state'), api('GET', '/api/interviews')]);
  const root = h('input', { type: 'text', value: state.library_root || '', placeholder: '/path/to/04_Fieldwork/Interviews', 'aria-label': 'Interview folder' });
  const scan = h('button', { type: 'submit', class: 'primary' }, 'Scan folder');
  const form = h('form', { class: 'panel', onsubmit: async (e) => {
    e.preventDefault();
    const res = await busy(scan, 'Scanning...', () => api('POST', '/api/library/scan', { root: root.value }));
    if (res) { toast(`${res.interviews.length} interviews in the library`, 'ok'); route(); }
  } },
  h('div', { class: 'row' },
    h('div', { class: 'grow' }, h('label', {}, 'Interview folder, one sub-folder per interview'), root),
    scan),
  h('p', { class: 'muted small', style: 'margin:8px 0 0' }, 'A rescan refreshes the inventory and records every added, removed or changed file with its SHA-256. Transcripts are read, never modified.'));

  const head = h('div', { class: 'page-head' },
    h('div', {}, h('h1', {}, 'Interviews'), h('div', { class: 'sub' }, `${list.length} in the library, codebook of ${state.codebook_size} codes`)));

  let body;
  if (!list.length) {
    body = emptyState('No interview yet.', 'Enter the folder that holds one sub-folder per interview, named with its ID (for example ECO-05_Vieweger), then scan it.');
  } else {
    body = h('div', { class: 'panel', style: 'padding:0; overflow:auto' }, h('table', {},
      h('thead', {}, h('tr', {}, h('th', {}, 'Interview'), h('th', {}, 'Folder'), h('th', {}, 'Pipeline'), h('th', {}, 'Next step'))),
      h('tbody', {}, list.map((s) => {
        const next = nextStep(s);
        return h('tr', {},
          h('td', {}, h('a', { href: `#/interview/${enc(s.interview_id)}`, class: 'mono' }, s.interview_id),
            s.n_warnings ? h('div', {}, h('span', { class: 'pill warn', title: 'See the Sources tab' }, `${s.n_warnings} warning${s.n_warnings > 1 ? 's' : ''}`)) : null),
          h('td', { class: 'small' }, s.folder_name),
          h('td', {}, ledger(s)),
          h('td', { class: 'nowrap' }, h('a', { href: `#/interview/${enc(s.interview_id)}/${next.tab}` }, next.label)));
      }))));
  }
  put(clear(view), head, form, body);
}

// ---------------------------------------------------------------- interview

const TABS = [
  ['sources', 'Sources'], ['coding', 'Coding'], ['ai', 'AI coding'], ['arbitration', 'Arbitration'], ['audit', 'Audit and export'],
];

async function viewInterview(id, tab) {
  const d = await api('GET', `/api/interviews/${enc(id)}`);
  const seg = d.segmentation;
  const nUnits = seg ? seg.units.length : 0;
  const nCoded = d.guard ? d.guard.n_units - d.guard.missing.length : 0;
  const tabs = h('nav', { class: 'tabs', 'aria-label': 'Interview steps' }, TABS.map(([key, label]) => {
    let count = null;
    if (key === 'coding' && seg) count = h('span', { class: 'count muted small' }, `${nCoded}/${nUnits}`);
    if (key === 'ai' && d.runs.length) count = h('span', { class: 'count muted small' }, `${d.runs.filter((r) => r.complete).length}/${d.runs.length}`);
    return h('a', { href: `#/interview/${enc(id)}/${key}`, class: key === tab ? 'active' : '' }, label, count);
  }));
  const head = h('div', { class: 'page-head' },
    h('div', {},
      h('div', { class: 'small' }, h('a', { href: '#/library' }, 'Interviews')),
      h('h1', { class: 'mono' }, d.entry.interview_id),
      h('div', { class: 'sub' }, d.entry.folder_name)));
  const body = h('div', {});
  put(clear(view), head, tabs, body);
  const render = { sources: tabSources, coding: tabCoding, ai: tabAi, arbitration: tabArbitration, audit: tabAudit }[tab] || tabSources;
  await render(d, body);
}

// ---------------------------------------------------------------- sources

async function tabSources(d, el) {
  const id = d.entry.interview_id;
  const viewer = h('div', {});
  const transcripts = d.entry.sources.filter((s) => TRANSCRIPT_KINDS.has(s.kind));

  async function showFile(path) {
    put(clear(viewer), h('div', { class: 'panel loading' }, 'Opening...'));
    try {
      const f = await api('GET', `/api/interviews/${enc(id)}/file?path=${enc(path)}`);
      const same = f.sha256_now === f.sha256_recorded;
      put(clear(viewer), h('div', { class: 'panel' },
        h('div', { class: 'row', style: 'justify-content:space-between; align-items:center; margin-bottom:8px' },
          h('h3', { class: 'mono' }, f.path),
          same ? h('span', { class: 'pill ok' }, 'Unchanged since the last scan')
               : h('span', { class: 'pill warn' }, 'Changed since the last scan, rescan the library')),
        h('pre', { class: 'viewer' }, f.text)));
      viewer.scrollIntoView({ block: 'start', behavior: 'smooth' });
    } catch (e) { clear(viewer); fail(e); }
  }

  const table = h('table', {},
    h('thead', {}, h('tr', {}, h('th', {}, 'Segment'), h('th', {}, 'Kind'), h('th', {}, 'File'), h('th', {}, 'SHA-256'), h('th', { class: 'num' }, 'Size'))),
    h('tbody', {}, d.entry.sources.map((s) => h('tr', {},
      h('td', {}, TRANSCRIPT_KINDS.has(s.kind)
        ? h('input', { type: 'radio', name: 'primary', 'aria-label': `Use ${s.rel_path}`, checked: d.entry.primary_source === s.rel_path,
            onchange: async () => {
              try { await api('POST', `/api/interviews/${enc(id)}/primary`, { rel_path: s.rel_path }); toast(`Primary transcript: ${s.rel_path}`, 'ok'); route(); }
              catch (e) { fail(e); }
            } })
        : null),
      h('td', { class: 'small' }, KIND_LABELS[s.kind] || s.kind),
      h('td', {}, h('button', { class: 'link mono', onclick: () => showFile(s.rel_path) }, s.rel_path)),
      h('td', { class: 'mono muted', title: s.sha256 }, s.sha256.slice(0, 12)),
      h('td', { class: 'num small' }, fmtSize(s.size))))));

  const warnings = d.entry.warnings.length
    ? h('details', { class: 'banner warn' }, h('summary', {}, `${d.entry.warnings.length} item(s) not inventoried or unreadable`),
        h('ul', {}, d.entry.warnings.map((w) => h('li', {}, w))))
    : null;

  const seg = d.segmentation;
  const segBtn = h('button', { class: 'primary', disabled: !d.entry.primary_source }, seg ? 'Segment again' : 'Segment the primary transcript');
  segBtn.addEventListener('click', async () => {
    const res = await busy(segBtn, 'Segmenting...', () => api('POST', `/api/interviews/${enc(id)}/segment`, { rel_path: d.entry.primary_source }));
    if (res) { toast(`Segmented: ${res.units.length} units`, 'ok'); route(); }
  });
  const segPanel = h('div', { class: 'panel' },
    h('h2', {}, 'Segmentation'),
    seg
      ? h('div', {},
          h('p', {}, `${seg.units.length} units from `, h('code', {}, seg.source_rel_path), '.'),
          h('p', { class: 'small muted' }, `Rule: ${seg.rule}.`),
          h('p', { class: 'small muted' }, 'Segmentation hash ', h('code', { title: seg.hash }, seg.hash.slice(0, 16)), ', source SHA-256 ', h('code', { title: seg.source_sha256 }, seg.source_sha256.slice(0, 16)), '.'),
          seg.warnings.length ? h('details', { class: 'small' }, h('summary', {}, `${seg.warnings.length} line(s) of the source not turned into units`), h('ul', {}, seg.warnings.map((w) => h('li', {}, w)))) : null,
          h('p', { class: 'small muted' }, 'Segmenting again is refused once coding has started, so the units every code refers to never change.'))
      : h('p', {}, d.entry.primary_source ? 'Not segmented yet. The rule is fixed: one unit per continuous speaker turn, turns over 150 words split at sentence ends.' : 'Choose the transcript to segment in the table below.'),
    segBtn);

  put(el, segPanel,
    transcripts.length ? null : h('div', { class: 'banner warn' }, 'No transcript was found in this folder. Supported: .vtt, raw transcript .txt, cleaned .md, Budgie Echo .md, written response .md.'),
    warnings,
    h('div', { class: 'panel', style: 'padding:0; overflow:auto' }, table),
    viewer);
}

// ----------------------------------------------------------------- coding

async function tabCoding(d, el) {
  const id = d.entry.interview_id;
  if (!d.segmentation) {
    put(el, emptyState('No units yet.', 'Segment a transcript in the Sources tab; its units appear here for coding.', h('a', { href: `#/interview/${enc(id)}/sources` }, 'Go to Sources')));
    return;
  }
  const rows = await api('GET', `/api/interviews/${enc(id)}/units`);
  const locked = d.human_locked;
  let sel = rows.findIndex((r) => !r.coding);
  if (sel < 0) sel = 0;
  let uncodedOnly = false;
  let draft = null;

  const progressBar = h('div', {});
  const progressText = h('span', { class: 'small nowrap' });
  const filter = h('input', { type: 'checkbox', id: 'uncoded-only', onchange: () => { uncodedOnly = filter.checked; renderList(); } });
  const addCode = h('button', { disabled: locked, onclick: async () => { if (await inductiveDialog()) renderSide(); } }, 'Add inductive code');
  const toolbar = h('div', { class: 'unit-toolbar' },
    h('div', { class: 'progress', 'aria-hidden': 'true' }, progressBar), progressText,
    h('label', { for: 'uncoded-only', style: 'display:flex; gap:6px; align-items:center; margin:0' }, filter, 'Uncoded only'),
    addCode,
    h('span', { class: 'muted small' }, h('span', { class: 'kbd' }, 'j'), ' ', h('span', { class: 'kbd' }, 'k'), ' move, ', h('span', { class: 'kbd' }, '/'), ' codes, ', h('span', { class: 'kbd' }, 'Ctrl Enter'), ' save and next'));

  const list = h('div', { class: 'units', role: 'list' });
  const side = h('aside', { class: 'side', 'aria-label': 'Code the selected unit' });
  const rowEls = new Map();

  function updateProgress() {
    const n = rows.filter((r) => r.coding && r.coding.codes.length).length;
    progressBar.style.width = `${(100 * n) / rows.length}%`;
    progressText.textContent = `${n} of ${rows.length} units coded`;
  }

  function unitRow(r, i) {
    const u = r.unit;
    const role = u.role === 'interviewer' ? 'int' : u.role === 'respondent' ? 'resp' : '';
    return h('div', {
      class: `unit ${role} ${r.coding ? '' : 'uncoded'} ${i === sel ? 'selected' : ''}`, role: 'listitem', tabindex: '-1',
      onclick: () => select(i),
    },
    h('div', { class: 'unit-meta' },
      h('span', { class: 'mono' }, u.unit_id.split('-').pop()),
      h('span', {}, u.timestamp),
      h('span', {}, h('span', { class: `spk ${role}`, title: u.speaker_raw }, role ? role.toUpperCase() : u.speaker_raw))),
    h('div', {}, h('div', { class: 'verbatim' }, u.text), r.coding ? chips(r.coding.codes) : null));
  }

  function renderList() {
    clear(list);
    rowEls.clear();
    rows.forEach((r, i) => {
      if (uncodedOnly && r.coding && i !== sel) return;
      const el = unitRow(r, i);
      rowEls.set(i, el);
      put(list, el);
    });
    if (!rowEls.size) put(list, h('div', { class: 'empty' }, h('strong', {}, 'Every unit is coded.'), 'Untick "Uncoded only" to review them.'));
    updateProgress();
  }

  function refreshRow(i) {
    const old = rowEls.get(i);
    if (!old) return;
    const fresh = unitRow(rows[i], i);
    old.replaceWith(fresh);
    rowEls.set(i, fresh);
  }

  function visibleIndexes() { return [...rowEls.keys()].sort((a, b) => a - b); }

  function select(i) {
    if (i < 0 || i >= rows.length) return;
    const prev = sel;
    sel = i;
    refreshRow(prev);
    if (!rowEls.has(i)) renderList(); else refreshRow(i);
    rowEls.get(i)?.scrollIntoView({ block: 'nearest' });
    renderSide();
  }

  function move(delta) {
    const vis = visibleIndexes();
    const pos = vis.indexOf(sel);
    const next = vis[Math.max(0, Math.min(vis.length - 1, (pos < 0 ? 0 : pos) + delta))];
    if (next !== undefined) select(next);
  }

  function renderSide() {
    const r = rows[sel];
    const u = r.unit;
    const c = r.coding || {};
    draft = {
      codes: new Set(c.codes || []),
    };
    const status = h('span', { class: 'small muted' }, r.coding ? 'Saved' : 'Not coded yet');
    const selectedBox = h('div', { class: 'codes', style: 'min-height:24px; margin-bottom:6px' });
    const naBtn = h('button', { type: 'button', disabled: locked });

    function refreshSelected() {
      clear(selectedBox);
      if (!draft.codes.size) put(selectedBox, h('span', { class: 'muted small' }, 'No code selected'));
      for (const code of [...draft.codes].sort()) {
        put(selectedBox, h('button', { type: 'button', class: 'link', title: 'Remove', disabled: locked, onclick: () => { toggle(code); picker.render(); } }, chip(code)));
      }
      naBtn.textContent = draft.codes.has('NA') ? 'NA selected: no code applies' : 'No code applies (NA)';
      naBtn.classList.toggle('primary', draft.codes.has('NA'));
      status.textContent = 'Unsaved changes';
    }
    function toggle(code) {
      if (code === 'NA') {
        draft.codes = draft.codes.has('NA') ? new Set() : new Set(['NA']);
      } else {
        draft.codes.delete('NA');
        if (draft.codes.has(code)) draft.codes.delete(code); else draft.codes.add(code);
      }
      refreshSelected();
    }
    naBtn.addEventListener('click', () => { toggle('NA'); picker.render(); });
    const picker = codePicker({ selected: () => draft.codes, onToggle: toggle, disabled: locked });

    const fld = (label, input) => h('div', { class: 'field' }, h('label', {}, label), input);
    const sel_ = (opts, value, name) => h('select', { 'aria-label': name, disabled: locked },
      h('option', { value: '' }, 'None'), opts.map((o) => h('option', { value: o, selected: o === value ? 'selected' : null }, o)));
    const subcode = h('input', { type: 'text', value: c.subcode || '', placeholder: 'facet-one, facet-two', disabled: locked });
    const modifier = sel_(MODIFIERS, c.modifier, 'Modifier');
    const ev = sel_(EV_LEVELS, c.ev_level, 'Evidence level');
    const prov = sel_(PROVENANCES, c.provenance, 'Provenance');
    const memo = h('textarea', { rows: 3, disabled: locked }, c.memo || '');
    const excerpt = h('textarea', { rows: 2, disabled: locked, placeholder: 'Select words in the verbatim above' }, c.excerpt || '');
    const text = h('div', { class: 'verbatim', title: 'Select words to use them as the verbatim excerpt' }, u.text);
    text.addEventListener('mouseup', () => {
      if (locked) return;
      const s = window.getSelection().toString().replace(/\s+/g, ' ').trim();
      if (s && u.text.includes(s)) { excerpt.value = s; status.textContent = 'Unsaved changes'; }
      else if (s) toast('The selection spans text that is not in this unit exactly; select within the unit.', 'error');
    });
    for (const input of [subcode, modifier, ev, prov, memo, excerpt]) input.addEventListener('input', () => { status.textContent = 'Unsaved changes'; });

    async function save(goNext) {
      if (locked) return;
      if (!draft.codes.size) { toast('Choose at least one code, or NA when no code applies.', 'error'); return; }
      const body = {
        codes: [...draft.codes], subcode: subcode.value, modifier: modifier.value, ev_level: ev.value,
        provenance: prov.value, memo: memo.value, excerpt: excerpt.value,
      };
      for (const k of Object.keys(body)) if (body[k] === '') body[k] = null;
      try {
        const res = await api('PUT', `/api/interviews/${enc(id)}/coding/${enc(u.unit_id)}`, body);
        rows[sel].coding = res.coding;
        refreshRow(sel);
        updateProgress();
        status.textContent = 'Saved';
        if (res.guard && res.guard.complete && res.guard.missing.length === 0 && rows.every((x) => x.coding)) {
          toast('Every unit is coded. The AI answer can now be imported.', 'ok');
        }
        if (goNext) {
          const vis = visibleIndexes();
          const after = vis.find((i) => i > sel && !rows[i].coding) ?? vis.find((i) => i > sel);
          if (after !== undefined) select(after); else renderSide();
        }
      } catch (e) { fail(e); }
    }
    side.onkeydown = (e) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); save(true); }
    };

    put(clear(side), 
      h('div', { class: 'row', style: 'justify-content:space-between; align-items:center; margin-bottom:8px' },
        h('h3', { class: 'mono' }, u.unit_id),
        h('span', { class: 'small muted' }, `${u.timestamp}, ${u.speaker_raw}`)),
      text,
      locked ? h('div', { class: 'banner info' }, 'Locked: an AI answer has been imported, so your primary coding is frozen. Disagreements are settled in Arbitration.') : null,
      h('label', {}, 'Codes'), selectedBox, naBtn, picker.el,
      h('div', { style: 'height:12px' }),
      h('div', { class: 'grid2' }, fld('Sub-code', subcode), fld('Modifier', modifier), fld('Evidence level', ev), fld('Provenance', prov)),
      fld('Analytical memo', memo),
      fld('Verbatim excerpt (exact words of the unit)', excerpt),
      locked ? null : h('div', { class: 'row', style: 'justify-content:space-between; align-items:center' },
        status,
        h('div', { class: 'row' },
          h('button', { type: 'button', onclick: () => save(false) }, 'Save'),
          h('button', { type: 'button', class: 'primary', onclick: () => save(true) }, 'Save and next'))));
    refreshSelected();
    status.textContent = r.coding ? 'Saved' : 'Not coded yet';
    side._picker = picker;
  }

  const onKey = (e) => {
    const tag = (e.target.tagName || '').toLowerCase();
    if (['input', 'textarea', 'select'].includes(tag) || e.target.isContentEditable) return;
    if (document.querySelector('dialog[open]')) return;
    if (e.key === 'j' || e.key === 'ArrowDown') { e.preventDefault(); move(1); }
    else if (e.key === 'k' || e.key === 'ArrowUp') { e.preventDefault(); move(-1); }
    else if (e.key === '/') { e.preventDefault(); side._picker?.search.focus(); }
  };
  document.addEventListener('keydown', onKey);
  cleanup = () => document.removeEventListener('keydown', onKey);

  put(el, 
    locked ? h('div', { class: 'banner info' }, 'Your coding of this interview is locked because an AI answer has been imported. This keeps the two codings independent; changes now go through Arbitration.') : null,
    toolbar,
    h('div', { class: 'coding' }, list, side));
  renderList();
  renderSide();
}

// --------------------------------------------------------------------- AI

function latestPromptSet(prompts) {
  if (!prompts.length) return [];
  const last = prompts[prompts.length - 1];
  const byPart = new Map();
  for (const p of prompts) if (p.of === last.of && p.segmentation_hash === last.segmentation_hash) byPart.set(p.part, p);
  return [...byPart.values()].sort((a, b) => a.part - b.part);
}

async function tabAi(d, el) {
  const id = d.entry.interview_id;
  if (!d.segmentation) {
    put(el, emptyState('No units yet.', 'The AI prompt is built from the units, so segment a transcript first.', h('a', { href: `#/interview/${enc(id)}/sources` }, 'Go to Sources')));
    return;
  }
  const g = d.guard;
  const guardBanner = g.complete
    ? h('div', { class: 'banner ok' }, `Your coding is complete (${g.n_units} units). AI answers can be imported.`)
    : h('div', { class: 'banner warn' }, `Blind lock: ${g.missing.length} of ${g.n_units} units still need your coding. You can generate and run the prompt now, but its answer is only accepted once your coding is complete, so that the two codings stay independent.`);

  // 1. Prompt
  const prompts = await api('GET', `/api/interviews/${enc(id)}/prompts`);
  let parts = latestPromptSet(prompts);
  const batch = h('input', { type: 'number', min: '0', step: '10', value: parts.length > 1 ? String(parts[0].unit_ids.length) : '0', style: 'width:120px' });
  const gen = h('button', { class: 'primary' }, parts.length ? 'Generate again' : 'Generate prompt');
  const partsBox = h('div', {});
  function renderParts() {
    clear(partsBox);
    for (const p of parts) {
      const ta = h('textarea', { readonly: true, rows: 10, 'aria-label': `Prompt part ${p.part}` }, p.text);
      put(partsBox, h('div', { class: 'prompt-part', style: 'margin-top:12px' },
        h('div', { class: 'row', style: 'justify-content:space-between; align-items:center; margin-bottom:6px' },
          h('strong', {}, `Part ${p.part} of ${p.of}, units ${p.unit_ids[0].split('-').pop()} to ${p.unit_ids[p.unit_ids.length - 1].split('-').pop()}`),
          h('span', { class: 'small muted' }, 'SHA-256 ', h('code', { title: p.sha256 }, p.sha256.slice(0, 16))),
          h('button', { onclick: async () => {
            try { await navigator.clipboard.writeText(p.text); toast(`Part ${p.part} copied`, 'ok'); }
            catch { ta.select(); document.execCommand('copy'); toast(`Part ${p.part} copied`, 'ok'); }
          } }, 'Copy prompt')),
        ta));
    }
  }
  gen.addEventListener('click', async () => {
    const res = await busy(gen, 'Generating...', () => api('POST', `/api/interviews/${enc(id)}/prompts`, { batch_size: Number(batch.value) || 0 }));
    if (res) { parts = res; renderParts(); renderImports(); toast(`${res.length} prompt part(s) generated and recorded`, 'ok'); }
  });
  renderParts();

  const promptPanel = h('div', { class: 'panel' },
    h('h2', {}, 'Generate the prompt'),
    h('p', { class: 'small muted' }, 'The prompt carries the codebook, the rules of blind coding and the units, nothing else: no codes of yours, no memo, no summary. It is identical for identical inputs, and each generated text is stored with its SHA-256.'),
    h('div', { class: 'row' },
      h('div', {}, h('label', {}, 'Units per prompt (0 for one prompt)'), batch),
      gen),
    partsBox);

  // 2. Run
  const runLabel = h('input', { type: 'text', value: `R${d.runs.length + 1}` });
  const model = h('input', { type: 'text', placeholder: 'e.g. claude-opus-5-5' });
  const version = h('input', { type: 'text', placeholder: 'version or date' });
  const temp = h('input', { type: 'number', step: '0.1', min: '0', value: '0' });
  const seed = h('input', { type: 'text' });
  const ctx = h('input', { type: 'number', min: '0' });
  const when = h('input', { type: 'datetime-local', value: nowLocalInput() });
  const notes = h('input', { type: 'text', placeholder: 'interface used, reworded prompt...' });
  const create = h('button', { type: 'submit', class: 'primary' }, 'Record the run');
  const runForm = h('form', { class: 'panel', onsubmit: async (e) => {
    e.preventDefault();
    const res = await busy(create, 'Recording...', () => api('POST', `/api/interviews/${enc(id)}/runs`, {
      run_label: runLabel.value, model_name: model.value, model_version: version.value,
      temperature: temp.value === '' ? null : Number(temp.value), seed: seed.value || null,
      context_length: ctx.value === '' ? null : Number(ctx.value),
      batch_size: parts.length ? parts[0].unit_ids.length : null,
      executed_at: when.value ? new Date(when.value).toISOString() : '', notes: notes.value || null,
    }));
    if (res) { toast(`Run ${res.meta.run_label} recorded`, 'ok'); route(); }
  } },
  h('h2', {}, 'Record the run'),
  h('p', { class: 'small muted' }, 'One run per execution of the prompt. P-04 asks for three: R1 main model, R2 reworded prompt, R3 a second, open-weight model. These parameters are journalised with the answer.'),
  h('div', { class: 'grid3' },
    h('div', { class: 'field' }, h('label', {}, 'Run label'), runLabel),
    h('div', { class: 'field' }, h('label', {}, 'Model name'), model),
    h('div', { class: 'field' }, h('label', {}, 'Model version'), version),
    h('div', { class: 'field' }, h('label', {}, 'Temperature'), temp),
    h('div', { class: 'field' }, h('label', {}, 'Seed'), seed),
    h('div', { class: 'field' }, h('label', {}, 'Context length'), ctx),
    h('div', { class: 'field' }, h('label', {}, 'Executed at'), when),
    h('div', { class: 'field', style: 'grid-column: span 2' }, h('label', {}, 'Notes'), notes)),
  create);

  // 3. Import
  const importBox = h('div', {});
  function renderImports() {
    clear(importBox);
    if (!d.runs.length) {
      put(importBox, h('p', { class: 'muted' }, 'Record a run first; each answer is attached to the run that produced it.'));
      return;
    }
    for (const run of d.runs) {
      const expected = run.parts_expected || (parts.length ? parts[0].of : 1);
      const received = new Set(run.parts_received);
      const partSel = h('select', { 'aria-label': 'Part' },
        Array.from({ length: expected }, (_, i) => i + 1).map((p) =>
          h('option', { value: String(p), disabled: received.has(p) ? 'disabled' : null }, `Part ${p} of ${expected}${received.has(p) ? ', imported' : ''}`)));
      const firstMissing = Array.from({ length: expected }, (_, i) => i + 1).find((p) => !received.has(p));
      if (firstMissing) partSel.value = String(firstMissing);
      const raw = h('textarea', { rows: 6, placeholder: 'Paste the AI answer here, exactly as returned', style: 'font-family: var(--mono); font-size: 0.8rem' });
      const issues = h('div', {});
      const imp = h('button', { class: 'primary' }, 'Import answer');
      imp.addEventListener('click', async () => {
        clear(issues);
        if (!raw.value.trim()) { toast('Paste the AI answer first.', 'error'); return; }
        imp.disabled = true;
        try {
          const res = await api('POST', `/api/interviews/${enc(id)}/runs/${run.run_id}/outputs`, { raw: raw.value, part: Number(partSel.value), of: expected });
          toast(`Part ${partSel.value} imported into ${run.meta.run_label}${res.complete ? ', run complete' : ''}`, 'ok');
          route();
        } catch (e) {
          if (e.issues && e.issues.length) {
            put(issues, h('div', { class: 'banner danger' }, e.message), h('ul', { class: 'issues' }, e.issues.map((i) => h('li', {}, h('code', {}, i.location), ' ', i.message))));
          } else if (e.kind === 'guard') {
            put(issues, h('div', { class: 'banner warn' }, e.message));
          } else fail(e);
        } finally { imp.disabled = false; }
      });
      const m = run.meta;
      put(importBox, h('div', { class: 'panel', style: 'margin-bottom:12px' },
        h('div', { class: 'row', style: 'justify-content:space-between; align-items:center; margin-bottom:8px' },
          h('h3', {}, `${m.run_label}, ${m.model_name}${m.model_version ? ` ${m.model_version}` : ''}`),
          h('span', { class: `pill ${run.complete ? 'ok' : run.parts_received.length ? 'warn' : ''}` },
            run.complete ? `Complete, ${run.n_coded} units` : `${run.parts_received.length} of ${expected} part(s) imported`)),
        h('p', { class: 'small muted' }, `Temperature ${m.temperature ?? 'not given'}, seed ${m.seed || 'not given'}, prompt ${m.prompt_id_version}, executed ${fmtTime(m.executed_at)}.`),
        run.complete ? null : h('div', {},
          h('div', { class: 'row' }, h('div', {}, h('label', {}, 'Answer to'), partSel)),
          h('div', { style: 'height:8px' }), raw,
          h('div', { class: 'row', style: 'margin-top:8px' }, imp),
          issues)));
    }
  }
  renderImports();

  put(el, guardBanner, promptPanel, runForm,
    h('div', { class: 'panel' }, h('h2', {}, 'Import the answers'),
      h('p', { class: 'small muted' }, 'The answer is checked strictly: right interview and segmentation, every unit once, only codebook codes or NA. It is stored verbatim with its SHA-256 before anything is derived from it.'),
      importBox));
}

// ------------------------------------------------------------- arbitration

async function tabArbitration(d, el) {
  const id = d.entry.interview_id;
  if (!d.segmentation) { put(el, emptyState('No units yet.', 'Segment, code and import an AI run first.')); return; }
  const [agr, dv] = await Promise.all([
    api('GET', `/api/interviews/${enc(id)}/agreement`),
    api('GET', `/api/interviews/${enc(id)}/divergences`),
  ]);
  if (!dv.runs.length) {
    put(el, emptyState('Nothing to compare yet.', 'Import an AI answer that covers every unit in the AI coding tab; its codes are then compared with yours here.', h('a', { href: `#/interview/${enc(id)}/ai` }, 'Go to AI coding')));
    return;
  }

  const agreeEls = agr.reports.map((r) => h('div', { style: 'margin-bottom:10px' },
    h('div', {}, h('strong', {}, `${r.label_a} and ${r.label_b}`), `: same codes on ${r.n_exact} of ${r.n_units} units (${pct(r.n_exact, r.n_units)}), ${r.divergent_unit_ids.length} divergent.`),
    h('details', { class: 'small' }, h('summary', {}, 'Agreement and Cohen\'s kappa per code'),
      h('table', {}, h('thead', {}, h('tr', {}, h('th', {}, 'Code'), h('th', { class: 'num' }, r.label_a), h('th', { class: 'num' }, r.label_b), h('th', { class: 'num' }, 'Raw agreement'), h('th', { class: 'num' }, 'Kappa'))),
        h('tbody', {}, r.per_code.map((c) => h('tr', {}, h('td', {}, chip(c.code)), h('td', { class: 'num' }, c.n_a), h('td', { class: 'num' }, c.n_b),
          h('td', { class: 'num' }, `${(100 * c.raw).toFixed(1)} %`), h('td', { class: 'num' }, c.kappa === null ? 'undefined' : c.kappa.toFixed(3)))))))));
  const pending = dv.n_divergences - dv.n_decided;
  let pendingOnly = pending > 0;
  const filter = h('input', { type: 'checkbox', id: 'pending-only', checked: pendingOnly, onchange: () => { pendingOnly = filter.checked; renderCards(); } });
  const cards = h('div', {});

  function renderCards() {
    clear(cards);
    const items = dv.items.filter((it) => !pendingOnly || !it.arbitration);
    if (!items.length) put(cards, emptyState(dv.n_divergences ? 'Every divergence is decided.' : 'No divergence.', dv.n_divergences ? 'Untick "Pending only" to review the decisions, or export the package from the Audit tab.' : 'Your coding and the AI runs agree on every unit.'));
    for (const it of items) put(cards, card(it));
  }

  function card(it) {
    const dvg = it.divergence;
    const u = it.unit;
    const arb = it.arbitration;
    const role = u.role === 'interviewer' ? 'int' : u.role === 'respondent' ? 'resp' : '';
    const runLabels = Object.keys(dvg.ai);
    const human = new Set(dvg.human);
    const aiSets = runLabels.map((l) => new Set(dvg.ai[l]));
    const compare = h('div', { class: 'compare' },
      h('span', { class: 'who' }, 'Your coding'), chips(dvg.human, (c) => aiSets.some((s) => !s.has(c))),
      runLabels.map((l, i) => [h('span', { class: 'who' }, `AI ${l}`), chips(dvg.ai[l], (c) => !human.has(c))]));
    const decisionBox = h('div', {});
    const wrap = h('article', { class: `div-card ${arb ? 'decided' : ''}` },
      h('header', {},
        h('span', { class: 'mono' }, u.unit_id), h('span', { class: 'small muted' }, u.timestamp),
        h('span', { class: `spk ${role}` }, role ? role.toUpperCase() : u.speaker_raw),
        dvg.kind === 'ai_runs_unstable' ? h('span', { class: 'pill warn', title: 'You agree with some runs, but the runs disagree among themselves' }, 'AI runs disagree') : null,
        h('span', { style: 'margin-left:auto', class: `pill ${arb ? 'ok' : 'warn'}` }, arb ? 'Decided' : 'Pending')),
      h('div', { class: 'body' }, h('div', { class: 'verbatim' }, u.text), compare, decisionBox));
    if (arb) showDecision(); else showForm();

    function showDecision() {
      const how = arb.choice.type === 'human' ? 'Kept your coding' : arb.choice.type === 'ai' ? `Took AI ${arb.choice.run_label}` : 'Chose other codes';
      put(clear(decisionBox), 
        h('div', { class: 'compare' }, h('span', { class: 'who' }, 'Final'), chips(arb.final_codes)),
        h('p', {}, h('span', { class: 'muted' }, `${how}. `), arb.rationale),
        h('p', { class: 'small muted' }, `Decided by ${arb.decided_by}, ${fmtTime(arb.decided_at)}.`),
        it.history.length > 1 ? h('details', { class: 'history' }, h('summary', {}, `${it.history.length} decisions recorded`),
          h('ol', {}, it.history.map((a) => h('li', {}, `${fmtTime(a.decided_at)}: ${[...a.final_codes].join('; ')}. ${a.rationale}`)))) : null,
        h('button', { onclick: showForm }, 'Revise the decision'));
    }

    function showForm() {
      let choice = null;
      let final = new Set();
      const buttons = [];
      const finalBox = h('div', {});
      const custom = h('div', {});
      const rationale = h('textarea', { rows: 3, placeholder: 'Why this coding is the right reading of the unit (required)' }, arb ? arb.rationale : '');
      const save = h('button', { class: 'primary' }, 'Record the decision');
      function pick(c, set, btn) {
        choice = c; final = new Set(set);
        buttons.forEach((b) => b.classList.toggle('on', b === btn));
        clear(custom);
        if (c.type === 'custom') {
          const p = codePicker({ selected: () => final, onToggle: (code) => {
            if (code === 'NA') final = final.has('NA') ? new Set() : new Set(['NA']);
            else { final.delete('NA'); final.has(code) ? final.delete(code) : final.add(code); }
            showFinal();
          } });
          put(custom, h('button', { type: 'button', style: 'margin:6px 0', onclick: () => { final = final.has('NA') ? new Set() : new Set(['NA']); p.render(); showFinal(); } }, 'No code applies (NA)'), p.el);
        }
        showFinal();
      }
      function showFinal() { put(clear(finalBox), h('div', { class: 'compare' }, h('span', { class: 'who' }, 'Final'), chips(final))); }
      const keep = h('button', { onclick: () => pick({ type: 'human' }, dvg.human, keep) }, 'Keep my coding');
      buttons.push(keep);
      for (const l of runLabels) {
        const b = h('button', { onclick: () => pick({ type: 'ai', run_label: l }, dvg.ai[l], b) }, `Take AI ${l}`);
        buttons.push(b);
      }
      const other = h('button', { onclick: () => pick({ type: 'custom' }, [...human], other) }, 'Other codes');
      buttons.push(other);
      save.addEventListener('click', async () => {
        if (!choice) { toast('Choose which coding to keep.', 'error'); return; }
        if (!final.size) { toast('The final coding needs at least one code, or NA.', 'error'); return; }
        if (!rationale.value.trim()) { toast('Write the rationale; every decision is justified in the adjudication log.', 'error'); rationale.focus(); return; }
        const res = await busy(save, 'Recording...', () => api('PUT', `/api/interviews/${enc(id)}/arbitration/${enc(u.unit_id)}`, { choice, final_codes: [...final], rationale: rationale.value }));
        if (res) {
          toast(`Decision recorded for ${u.unit_id}`, 'ok');
          const fresh = await api('GET', `/api/interviews/${enc(id)}/divergences`);
          Object.assign(dv, fresh);
          updateCounter();
          renderCards();
        }
      });
      put(clear(decisionBox), 
        h('div', { class: 'choices' }, buttons), custom, finalBox,
        h('div', { class: 'field' }, h('label', {}, 'Rationale'), rationale),
        h('div', { class: 'row' }, save, arb ? h('button', { onclick: showDecision }, 'Cancel') : null));
    }
    return wrap;
  }

  const counter = h('span', {});
  function updateCounter() {
    const p = dv.n_divergences - dv.n_decided;
    counter.textContent = `${dv.n_divergences} divergence${dv.n_divergences === 1 ? '' : 's'}: ${dv.n_decided} decided, ${p} pending.`;
  }
  updateCounter();
  put(el, 
    h('div', { class: 'panel' }, h('h2', {}, 'Agreement'), agreeEls,
      h('p', { class: 'small muted', style: 'margin:0' }, 'Computed without any model: exact agreement on the full set of codes, then Cohen\'s kappa per code on presence or absence.')),
    h('div', { class: 'row', style: 'align-items:center; margin-bottom:12px' },
      h('h2', {}, 'Divergences'), counter,
      h('label', { for: 'pending-only', style: 'display:flex; gap:6px; align-items:center; margin:0 0 0 auto' }, filter, 'Pending only')),
    h('p', { class: 'small muted' }, 'A unit diverges when your codes differ from an AI run, or when the AI runs disagree among themselves. Every divergence is decided by hand, with a written rationale; earlier decisions stay in the history.'),
    cards);
  renderCards();
}

// ------------------------------------------------------------------- audit

function auditTable(events, { showInterview = false } = {}) {
  const LIMIT = 300;
  const box = h('div', { class: 'panel audit', style: 'padding:0; overflow:auto' });
  let all = false;
  function render() {
    const rev = [...events].reverse();
    const shown = all ? rev : rev.slice(0, LIMIT);
    put(clear(box), h('table', {},
      h('thead', {}, h('tr', {}, h('th', { class: 'num' }, 'Seq'), h('th', {}, 'Time'), h('th', {}, 'Actor'), h('th', {}, 'Action'),
        showInterview ? h('th', {}, 'Interview') : null, h('th', {}, 'Target'), h('th', {}, 'Detail'))),
      h('tbody', {}, shown.map((e) => h('tr', {},
        h('td', { class: 'num mono' }, e.seq), h('td', { class: 'nowrap small' }, fmtTime(e.ts)), h('td', { class: 'small' }, e.actor),
        h('td', {}, h('code', {}, e.action)),
        showInterview ? h('td', { class: 'mono small' }, e.interview_id || '') : null,
        h('td', { class: 'mono small' }, e.target || ''),
        h('td', { class: 'detail' }, h('details', {}, h('summary', { class: 'small' }, `hash ${e.hash.slice(0, 12)}`),
          h('pre', {}, JSON.stringify(e.detail, null, 2)), h('div', { class: 'small muted mono' }, `prev ${e.prev_hash.slice(0, 16)}`))))))));
    if (rev.length > LIMIT && !all) put(box, h('div', { style: 'padding:10px' }, h('button', { onclick: () => { all = true; render(); } }, `Show all ${rev.length} events`)));
    if (!rev.length) put(box, h('div', { class: 'empty' }, 'No event recorded yet.'));
  }
  render();
  return box;
}

async function tabAudit(d, el) {
  const id = d.entry.interview_id;
  const [events, chain] = await Promise.all([api('GET', `/api/audit?interview_id=${enc(id)}`), api('GET', '/api/audit/verify')]);
  const result = h('div', {});
  const exp = h('button', { class: 'primary', disabled: !d.segmentation }, 'Export the audit package');
  exp.addEventListener('click', async () => {
    const res = await busy(exp, 'Exporting...', () => api('POST', `/api/interviews/${enc(id)}/export`));
    if (!res) return;
    toast(`Package written: ${res.files.length} files`, 'ok');
    put(clear(result), 
      h('div', { class: `banner ${res.final_complete ? 'ok' : 'warn'}` },
        res.final_complete ? 'Final coding included: every divergence is arbitrated.' : res.pending_units.length ? `Final coding not included: ${res.pending_units.length} unit(s) still pending.` : 'No complete AI run yet: the package holds the units and your coding.'),
      h('p', {}, 'Written to ', h('code', {}, res.dir), '. Manifest SHA-256 ', h('code', {}, res.manifest_sha256.slice(0, 16)), '.'),
      h('table', {}, h('thead', {}, h('tr', {}, h('th', {}, 'File'), h('th', {}, 'SHA-256'), h('th', { class: 'num' }, 'Size'))),
        h('tbody', {}, res.files.map((f) => h('tr', {}, h('td', { class: 'mono small' }, f.name), h('td', { class: 'mono small muted', title: f.sha256 }, f.sha256.slice(0, 16)), h('td', { class: 'num small' }, fmtSize(f.size)))))));
  });
  put(el, 
    h('div', { class: 'panel' },
      h('h2', {}, 'Export'),
      h('p', { class: 'small muted' }, 'Writes a new dated folder next to the database with the units, both codings, every prompt and raw AI answer, the agreement report, the adjudication log, the final coding when complete, this interview\'s audit events and a manifest of SHA-256 hashes. Nothing is ever overwritten.'),
      exp, result),
    h('div', { class: 'row', style: 'align-items:center; margin-bottom:12px' },
      h('h2', {}, 'Recorded actions'),
      h('span', { class: `pill ${chain.ok ? 'ok' : 'danger'}` }, chain.ok ? `Chain intact, ${chain.n_events} events in the database` : `Chain broken at event ${chain.first_bad_seq}`)),
    auditTable(events));
}

// ---------------------------------------------------------------- codebook

async function viewCodebook() {
  const [state, book] = await Promise.all([api('GET', '/api/state'), loadBook(true)]);
  const path = h('input', { type: 'text', value: state.coding_db || '', placeholder: '/path/to/05_Analysis/coding/coding.db', 'aria-label': 'coding.db path' });
  const imp = h('button', { type: 'submit' }, 'Import from coding.db');
  const add = h('button', { type: 'button', onclick: async () => { if (await inductiveDialog()) route(); } }, 'Add inductive code');
  const offered = book.codes.filter((c) => c.active && c.in_prompt).length;
  const families = [...new Set(book.codes.map((c) => c.family))];
  const head = h('div', { class: 'page-head' },
    h('div', {}, h('h1', {}, 'Codebook'), h('div', { class: 'sub' }, `${book.codes.length} codes from ${book.source || 'no source yet'}, ${offered} offered to the AI coder`)));
  const form = h('form', { class: 'panel', onsubmit: async (e) => {
    e.preventDefault();
    const res = await busy(imp, 'Importing...', () => api('POST', '/api/codebook/import', { coding_db: path.value }));
    if (res) { BOOK = res; toast(`${res.codes.length} codes imported`, 'ok'); route(); }
  } },
  h('div', { class: 'row' }, h('div', { class: 'grow' }, h('label', {}, 'Thesis coding.db, opened read only'), path), imp, add),
  h('p', { class: 'small muted', style: 'margin:8px 0 0' }, 'Importing again keeps your inductive codes and your choices of what is offered to the AI. A code removed upstream is deactivated, never deleted. Legacy family codes are not offered to the AI by default.'));
  if (!book.codes.length) {
    put(clear(view), head, form, emptyState('The codebook is empty.', 'Import it from the thesis coding.db, which stays untouched.'));
    return;
  }
  const table = h('table', {},
    h('thead', {}, h('tr', {}, h('th', {}, 'Code'), h('th', {}, 'Label'), h('th', {}, 'Definition'), h('th', {}, 'Origin'), h('th', {}, 'Offered to AI'))),
    families.map((fam) => h('tbody', {},
      h('tr', {}, h('th', { colspan: '5', style: 'background:var(--bg)' }, fam)),
      book.codes.filter((c) => c.family === fam).map((c) => h('tr', { style: c.active ? '' : 'opacity:.55' },
        h('td', {}, chip(c.code), c.active ? null : h('div', { class: 'small muted' }, 'inactive')),
        h('td', {}, c.label),
        h('td', { class: 'small', style: 'max-width:60ch' }, c.definition),
        h('td', { class: 'small muted' }, c.origin),
        h('td', {}, h('input', { type: 'checkbox', checked: c.in_prompt, disabled: !c.active, 'aria-label': `Offer ${c.code} to the AI`,
          onchange: async (e) => {
            try { BOOK = await api('PUT', `/api/codebook/codes/${enc(c.code)}/in_prompt`, { in_prompt: e.target.checked }); toast(`${c.code} ${e.target.checked ? 'offered to' : 'withheld from'} the AI coder`, 'ok'); }
            catch (err) { e.target.checked = !e.target.checked; fail(err); }
          } })))))));
  put(clear(view), head, form, h('div', { class: 'panel', style: 'padding:0; overflow:auto' }, table));
}

// ------------------------------------------------------------- global audit

async function viewAudit() {
  const [events, chain] = await Promise.all([api('GET', '/api/audit'), api('GET', '/api/audit/verify')]);
  const verify = h('button', { onclick: async () => {
    const c = await busy(verify, 'Verifying...', () => api('GET', '/api/audit/verify'));
    if (c) { toast(c.ok ? `Chain intact: ${c.n_events} events verified` : `Chain broken at event ${c.first_bad_seq}`, c.ok ? 'ok' : 'error'); refreshChain(); }
  } }, 'Verify the chain again');
  put(clear(view), 
    h('div', { class: 'page-head' }, h('div', {}, h('h1', {}, 'Audit trail'),
      h('div', { class: 'sub' }, 'Every action, append only. Each event carries the SHA-256 of the previous one, so any later edit breaks the chain.'))),
    h('div', { class: `banner ${chain.ok ? 'ok' : 'danger'}` },
      chain.ok ? `Chain intact: ${chain.n_events} events, every hash recomputed and matching.` : `Chain broken at event ${chain.first_bad_seq}: the database was modified outside the tool.`),
    h('div', { style: 'margin-bottom:12px' }, verify),
    auditTable(events, { showInterview: true }));
}

route();

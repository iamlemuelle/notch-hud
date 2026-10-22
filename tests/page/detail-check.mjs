// Loads dist/index.html in headless Chromium with a fake Tauri bridge and checks CONTRACT.md section 4 "Detail card" and
// section 8 "Detail (amendment)": clickable rows, the card and its four fields, the hit shape and clip while the card
// shows, the refetch on every `sessions` event, `Session ended`, back, and the reset to the list on collapse. Every
// expected value is a literal written from the contract text. Real time, no retries.
//
// Env: PLAYWRIGHT_CORE, CHROMIUM, NOTCH_PAGE as in tests/page_test.sh. Exit 0 when every check passes, 1 when one
// fails, 2 when the browser cannot start.
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PW = process.env.PLAYWRIGHT_CORE || '/Users/lem/.npm/_npx/a1722018f28f9df2/node_modules/playwright-core';
const CHROMIUM = process.env.CHROMIUM || '/Users/lem/Library/Caches/ms-playwright/chromium_headless_shell-1208/chrome-headless-shell-mac-arm64/chrome-headless-shell';
const PAGE_FILE = resolve(process.env.NOTCH_PAGE || join(repo, 'dist', 'index.html'));

let chromium;
let browser;
try {
  ({ chromium } = require(PW));
  browser = await chromium.launch({ executablePath: CHROMIUM });
} catch (e) {
  console.log(`SKIP cannot start the browser (PLAYWRIGHT_CORE=${PW}, CHROMIUM=${CHROMIUM}): ${e.message}`);
  process.exit(2);
}
setTimeout(() => { console.log('FAIL watchdog: the run took longer than 240 s'); process.exit(1); }, 240000).unref();

// Contract numbers (section 8): stage 452 x 982, pill 305 x 32, open 420 x 254 for one row (a "N more" row adds 22),
// detail 420 x 380 (radius 22), the open window 452 x 396.
const LAYOUT = {
  stage: { width: 452, height: 982 },
  pill: { width: 305, height: 32 },
  peek: { width: 309, height: 36 },
  banner: { width: 380, height: 78 },
  open: { width: 420, height: 342 },
  detail: { width: 420, height: 380 },
  ear: 60,
};
const WINDOW = { pill: { w: 305, h: 32 }, banner: { w: 412, h: 94 }, open: { w: 452, h: 396 } };
const STAGE = { w: 452, h: 982 };
const AMBER = 'rgb(245, 166, 35)';

const results = [];
const check = (name, ok, detail) => results.push({ name, ok: !!ok, detail: ok ? undefined : detail });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const J = (v) => JSON.stringify(v);
const near = (v, target, tol) => typeof v === 'number' && Math.abs(v - target) <= tol;
const pageErrors = [];

// Installed before any page script: the fake bridge. `session_detail` answers with window.__reply(id) (null by default).
function installBridge({ layout, windows }) {
  const handlers = {};
  const rec = (window.__rec = { invokes: [] });
  window.__reply = () => null;
  window.__fire = (name, payload) => { (handlers[name] || []).forEach((h) => h({ payload })); };
  window.__TAURI__ = {
    event: { listen: (name, h) => { (handlers[name] = handlers[name] || []).push(h); return Promise.resolve(() => {}); } },
    core: {
      invoke: (cmd, args) => {
        rec.invokes.push({ cmd, args: args ? JSON.parse(JSON.stringify(args)) : null });
        if (cmd === 'layout') return Promise.resolve(layout);
        if (cmd === 'session_detail') return Promise.resolve(window.__reply(args && args.id));
        if (cmd === 'surface') {
          const w = args && windows[args.window];
          return new Promise((resolve) => setTimeout(async () => { if (w) await window.__setWindow(w.w, w.h); resolve(null); }, 0));
        }
        return Promise.resolve(null);
      },
    },
  };
  // A visible box: opacity of the element and its ancestors, display, visibility.
  window.__vis = (sel) => {
    const e = document.querySelector(sel);
    if (!e) return null;
    let eff = 1;
    for (let n = e; n && n.nodeType === 1; n = n.parentElement) {
      const c = getComputedStyle(n);
      if (c.display === 'none' || c.visibility === 'hidden' || n.hidden) { eff = 0; break; }
      eff *= parseFloat(c.opacity);
    }
    const r = e.getBoundingClientRect();
    return { eff: r.width > 0 && r.height > 0 ? eff : 0, text: e.textContent.replace(/\s+/g, ' ').trim(), cls: e.className, color: getComputedStyle(e).color };
  };
}

async function openPage(opts = {}) {
  const page = await browser.newPage({ viewport: { width: WINDOW.pill.w, height: WINDOW.pill.h } });
  const tag = opts.tag || 'page';
  page.on('pageerror', (e) => pageErrors.push(`${tag}: pageerror: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error') pageErrors.push(`${tag}: console: ${m.text()}`); });
  if (opts.reduced) await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.exposeFunction('__setWindow', (w, h) => page.setViewportSize({ width: w, height: h }).catch(() => {}));
  await page.addInitScript(installBridge, { layout: LAYOUT, windows: WINDOW });
  await page.goto('file://' + PAGE_FILE);
  await page.waitForFunction(() => document.body && document.body.dataset.phase, null, { timeout: 1500 }).catch(() => {});
  return page;
}

const fire = (page, name, payload) => page.evaluate(([n, p]) => window.__fire(n, p), [name, payload]);
const vis = (page, sel) => page.evaluate((s) => window.__vis(s), sel);
const waitPhase = (page, phase, ms) => page.waitForFunction((p) => document.body.dataset.phase === p, phase, { timeout: ms, polling: 20 }).then(() => true, () => false);
const waitTransitionsDone = (page, ms) => page.waitForFunction(() => document.getAnimations().every((a) => !(a instanceof CSSTransition) || a.playState !== 'running'), null, { timeout: ms, polling: 20 }).then(() => true, () => false);
const invokes = (page, cmd) => page.evaluate((c) => window.__rec.invokes.filter((i) => i.cmd === c).map((i) => i.args), cmd);
const setReply = (page, detail) => page.evaluate((d) => { window.__reply = typeof d === 'function' ? d : () => d; }, detail);
const nowSec = () => Math.floor(Date.now() / 1000);

const row = (id, label, status, detail = null) => ({ id, label, status, detail });
const summary = (rows, total = rows.length) => ({
  rows, total,
  attention: rows.filter((r) => ['needs_permission', 'needs_input', 'error'].includes(r.status)).length,
  working: rows.filter((r) => r.status === 'working').length,
});
const ROWS = [
  row('id-one', 'alpha', 'needs_permission', 'Bash'),
  row('id-two', 'bravo', 'working'),
  row('id-three', 'charlie', 'done'),
  row('id-four', 'delta', 'waiting'),
];
const SUMMARY = summary(ROWS, 6);
const card = (over = {}) => ({
  id: 'id-two', label: 'bravo', cwd: '/Users/lem/dev/bravo', status: 'working', event: 'PreToolUse',
  event_ts: nowSec() - 4, tool_name: 'Bash', agents: 2, ...over,
});

// "inset(0px 16px 602px round 0px 0px 22px 22px)" to the visible size and the bottom radii.
function parseClip(s) {
  const m = /^inset\((.+?)(?:\s+round\s+(.+))?\)$/.exec(String(s ?? '').trim());
  if (!m) return null;
  const four = (list) => {
    const v = list.trim().split(/\s+/).map(parseFloat);
    if (v.some(Number.isNaN)) return null;
    return v.length === 1 ? [v[0], v[0], v[0], v[0]] : v.length === 2 ? [v[0], v[1], v[0], v[1]] : v.length === 3 ? [v[0], v[1], v[2], v[1]] : v.length === 4 ? v : null;
  };
  const e = four(m[1]);
  const r = m[2] ? four(m[2]) : [0, 0, 0, 0];
  if (!e || !r) return null;
  return { w: STAGE.w - e[1] - e[3], h: STAGE.h - e[0] - e[2], bottom: [r[2], r[3]] };
}
const clipOf = (page) => page.evaluate(() => getComputedStyle(document.querySelector('#panel')).clipPath);

// The fields of the card: [{name, value, text}] from #dRows .kv.
const fields = (page) => page.evaluate(() => [...document.querySelectorAll('#dRows .kv')].map((kv) => ({
  name: (kv.querySelector('dt') || {}).textContent, value: ((kv.querySelector('dd') || {}).textContent || '').replace(/\s+/g, ' ').trim(),
  color: kv.querySelector('dd') && kv.querySelector('dd').lastElementChild ? getComputedStyle(kv.querySelector('dd').lastElementChild).color : null,
})));
const field = async (page, name) => (await fields(page)).find((f) => f.name === name);

// Hover in and wait for the expanded phase and its spring.
async function expand(page) {
  await fire(page, 'hover', true);
  const ok = await waitPhase(page, 'expanded', 2500);
  await waitTransitionsDone(page, 1500);
  return ok;
}
async function collapse(page) {
  await fire(page, 'hover', false);
  const ok = await waitPhase(page, 'collapsed', 2500);
  await waitTransitionsDone(page, 1500);
  return ok;
}
// Opens the card on row 2 with a real click.
async function openCard(page, reply = card()) {
  await setReply(page, reply);
  await page.locator('#rows button.srow').nth(1).click({ timeout: 2000 });
  await page.waitForFunction(() => { const d = document.querySelector('#detail'); return d && !d.classList.contains('off'); }, null, { timeout: 2000, polling: 20 }).catch(() => {});
  await waitTransitionsDone(page, 1500);
  await sleep(700); // the 366 ms clip spring and the 120 ms crossfade
}
const lastSurface = async (page) => (await invokes(page, 'surface')).pop();

// Scenarios ------------------------------------------------------------------------------------------------------

async function scenarioRows() {
  const page = await openPage({ tag: 'rows' });
  await fire(page, 'sessions', SUMMARY);
  await sleep(100);
  const info = await page.evaluate(() => ({
    buttons: [...document.querySelectorAll('#rows button.srow[data-id]')].map((b) => ({ id: b.dataset.id, type: b.type })),
    more: [...document.querySelectorAll('#rows div.srow.more')].map((d) => ({ tag: d.tagName, text: d.textContent.replace(/\s+/g, ' ').trim() })),
    anyMoreButton: !!document.querySelector('#rows button.more'),
    all: document.querySelectorAll('#rows .srow').length,
  }));
  check('R1 each row is a button.srow with data-id', J(info.buttons) === J(ROWS.map((r) => ({ id: r.id, type: 'button' }))), J(info.buttons));
  check('R2 the "N more" row is a div.srow.more, not a button', info.more.length === 1 && info.more[0].tag === 'DIV' && info.more[0].text === '2 more' && !info.anyMoreButton && info.all === 5, J(info));
  await fire(page, 'sessions', summary([]));
  await sleep(100);
  const none = await page.evaluate(() => ({
    buttons: document.querySelectorAll('#rows button').length,
    more: [...document.querySelectorAll('#rows div.srow.more')].map((d) => d.textContent.replace(/\s+/g, ' ').trim()),
  }));
  check('R3 "No Claude Code sessions" is a div.srow.more and there are no buttons', none.buttons === 0 && J(none.more) === J(['No Claude Code sessions']), J(none));
  await page.close();
}

async function scenarioOpen() {
  const page = await openPage({ tag: 'open' });
  await fire(page, 'sessions', SUMMARY);
  check('O0 expands', await expand(page), 'phase never became expanded');
  const before = await vis(page, '#sess');
  check('O1 the list shows first, the card is hidden (class off)', before && before.eff > 0.9 && (await vis(page, '#detail')).eff < 0.01 && /\boff\b/.test((await vis(page, '#detail')).cls), J({ sess: before, detail: await vis(page, '#detail') }));
  const surfacesBefore = (await invokes(page, 'surface')).length;
  await openCard(page);
  const calls = await invokes(page, 'session_detail');
  check('O2 a real click on row 2 calls session_detail with row 2\'s id', calls.length === 1 && J(calls[0]) === J({ id: 'id-two' }), J(calls));
  const d = await vis(page, '#detail'), s = await vis(page, '#sess');
  check('O3 the card shows and the list is hidden (class off)', d.eff > 0.9 && s.eff < 0.01 && /\boff\b/.test(s.cls), J({ d, s }));
  const surfaces = await invokes(page, 'surface');
  check('O4 surface was called with {window: "open", hit: "detail"} after the card showed', surfaces.length > surfacesBefore && J(surfaces[surfaces.length - 1]) === J({ window: 'open', hit: 'detail' }), J(surfaces));
  const clip = parseClip(await clipOf(page));
  check('O5 the clip is the detail shape, 420 x 380 with bottom radius 22', clip && near(clip.w, 420, 0.6) && near(clip.h, 380, 0.6) && clip.bottom.every((r) => near(r, 22, 0.1)), J(clip));
  check('O6 the header shows the label in #dLabel and the back control', (await vis(page, '#dLabel')).text === 'bravo' && (await vis(page, '#back')).eff > 0.9, J({ l: await vis(page, '#dLabel'), b: await vis(page, '#back') }));
  const back = await page.evaluate(() => { const b = document.querySelector('#back'); return { cls: b.className, label: b.getAttribute('aria-label'), type: b.type, tag: b.tagName }; });
  check('O7 the back control is button.back with aria-label "Back to all sessions"', back.tag === 'BUTTON' && back.type === 'button' && /\bback\b/.test(back.cls) && back.label === 'Back to all sessions', J(back));
  await page.close();
}

async function scenarioFields() {
  const page = await openPage({ tag: 'fields' });
  await fire(page, 'sessions', SUMMARY);
  await expand(page);
  await openCard(page, card({ status: 'needs_permission', event: 'PermissionRequest', tool_name: 'Edit' }));
  const f = await fields(page);
  check('F1 four rows named Status, Folder, Last event, Subagents', J(f.map((x) => x.name)) === J(['Status', 'Folder', 'Last event', 'Subagents']), J(f));
  const status = f[0];
  check('F2 Status: "needs permission" without the tool, in amber', status && status.value === 'needs permission' && status.color === AMBER, J(status));
  check('F3 Folder: /Users/<name> shown as ~', f[1] && f[1].value === '~/dev/bravo', J(f[1]));
  check('F4 Last event: the event words, then an age like "4 s ago"', f[2] && /^Asked to use Edit ?[45] s ago$/.test(f[2].value), J(f[2]));
  check('F5 Subagents: "2 running"', f[3] && f[3].value === '2 running', J(f[3]));
  const dot = await page.evaluate(() => !!document.querySelector('#dRows .kv .dot, #dRows .kv [class*="dot"]'));
  check('F6 the Status row carries a dot', dot, 'no dot element in the Status row');

  const folder = async (cwd) => { await setReply(page, card({ cwd })); await fire(page, 'sessions', SUMMARY); await sleep(150); return (await field(page, 'Folder')).value; };
  check('F7 Folder: "/Users/lem" alone is "~"', (await folder('/Users/lem')) === '~', await field(page, 'Folder'));
  check('F8 Folder: a path outside /Users is shown as is', (await folder('/opt/work/tool')) === '/opt/work/tool', await field(page, 'Folder'));
  check('F8b Folder: /Users/Shared is not a home folder', (await folder('/Users/Shared/x')) === '/Users/Shared/x', await field(page, 'Folder'));

  const words = [
    ['SessionStart', null, 'Started'], ['UserPromptSubmit', null, 'Got a prompt'], ['PreToolUse', 'Bash', 'Running Bash'],
    ['PreToolUse', null, 'Running a tool'], ['PostToolUse', 'Read', 'Read finished'], ['PostToolUseFailure', 'Edit', 'Edit failed'],
    ['PermissionRequest', 'Bash', 'Asked to use Bash'], ['PermissionRequest', null, 'Asked to use a tool'], ['PermissionDenied', null, 'Permission denied'],
    ['Notification', null, 'Waiting for your answer'], ['Stop', null, 'Finished its turn'], ['StopFailure', null, 'Stopped with an error'],
    ['SomethingNew', null, 'SomethingNew'],
  ];
  const bad = [];
  for (const [event, tool_name, want] of words) {
    await setReply(page, card({ event, tool_name }));
    await fire(page, 'sessions', SUMMARY);
    await sleep(120);
    const got = (await field(page, 'Last event')).value;
    if (!new RegExp(`^${want} ?[45] s ago$`).test(got)) bad.push(`${event}/${tool_name}: ${got}`);
  }
  check('F9 Last event words for every event of the contract table', bad.length === 0, J(bad));

  const ages = [[nowSec() - 125, /2 min ago$/], [nowSec() - 7300, /2 h ago$/], [nowSec() + 500, / 0 s ago$|Bash0 s ago$/], [nowSec() - 59, /(59|60) s ago$|1 min ago$/], [nowSec() - 3599, /(59 min|1 h) ago$/]];
  const badAge = [];
  for (const [event_ts, re] of ages) {
    await setReply(page, card({ event_ts }));
    await fire(page, 'sessions', SUMMARY);
    await sleep(120);
    const got = (await field(page, 'Last event')).value;
    if (!re.test(got)) badAge.push(`${event_ts}: ${got}`);
  }
  check('F10 ages: min, h, a future event_ts is "0 s ago"', badAge.length === 0, J(badAge));

  await setReply(page, card({ agents: 0, status: 'working' }));
  await fire(page, 'sessions', SUMMARY);
  await sleep(120);
  check('F11 Subagents with 0 is "None"', (await field(page, 'Subagents')).value === 'None', J(await field(page, 'Subagents')));
  const bad2 = [];
  for (const [status, want, amber] of [['working', 'working', false], ['done', 'done', false], ['waiting', 'waiting for you', false], ['error', 'error', true], ['needs_input', 'needs your input', true]]) {
    await setReply(page, card({ status }));
    await fire(page, 'sessions', SUMMARY);
    await sleep(120);
    const s = await field(page, 'Status');
    if (s.value !== want || (s.color === AMBER) !== amber) bad2.push(`${status}: ${J(s)}`);
  }
  check('F12 Status text and color for the other statuses', bad2.length === 0, J(bad2));

  // The age is redrawn every second while the card shows expanded.
  await setReply(page, card({ event_ts: nowSec() - 10 }));
  await fire(page, 'sessions', SUMMARY);
  await sleep(150);
  const a1 = await page.evaluate(() => document.querySelector('#dRows .age').textContent.trim());
  await sleep(2300);
  const a2 = await page.evaluate(() => document.querySelector('#dRows .age').textContent.trim());
  const n1 = parseInt(a1, 10), n2 = parseInt(a2, 10);
  check('F13 #dRows .age is redrawn every second', /^\d+ s ago$/.test(a1) && /^\d+ s ago$/.test(a2) && n2 - n1 >= 2, J({ a1, a2 }));
  await page.close();
}

async function scenarioRefetch() {
  const page = await openPage({ tag: 'refetch' });
  await fire(page, 'sessions', SUMMARY);
  await expand(page);
  await openCard(page);
  const n0 = (await invokes(page, 'session_detail')).length;
  await setReply(page, card({ label: 'bravo', status: 'done', event: 'Stop', tool_name: null, agents: 0 }));
  await fire(page, 'sessions', SUMMARY);
  await sleep(200);
  const calls = await invokes(page, 'session_detail');
  check('X1 a sessions event refetches with the same id', calls.length === n0 + 1 && J(calls[calls.length - 1]) === J({ id: 'id-two' }), J(calls));
  check('X2 the card is redrawn from the reply', (await field(page, 'Status')).value === 'done' && /^Finished its turn/.test((await field(page, 'Last event')).value), J(await fields(page)));

  // A `records` event (no payload, no `sessions` event) refetches and redraws: the newest event and the subagents change.
  const nr = (await invokes(page, 'session_detail')).length;
  await setReply(page, card({ status: 'done', event: 'PostToolUse', tool_name: 'Grep', agents: 2 }));
  await fire(page, 'records', undefined);
  await sleep(200);
  const rcalls = await invokes(page, 'session_detail');
  check('X1b a records event refetches with the same id', rcalls.length === nr + 1 && J(rcalls[rcalls.length - 1]) === J({ id: 'id-two' }), J(rcalls));
  check('X2b the card is redrawn from the reply to a records event', /^Grep finished ?[45] s ago$/.test((await field(page, 'Last event')).value) && (await field(page, 'Subagents')).value === '2 running', J(await fields(page)));

  await setReply(page, null);
  await fire(page, 'sessions', SUMMARY);
  await sleep(250);
  const ended = await vis(page, '#dEnded');
  const kv = await page.evaluate(() => [...document.querySelectorAll('#dRows .kv')].filter((e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0 && getComputedStyle(e).opacity !== '0'; }).length);
  check('X3 a null reply shows "Session ended" and no rows', ended && ended.eff > 0.9 && ended.text === 'Session ended' && kv === 0, J({ ended, kv }));
  check('X4 the back control and the last label stay', (await vis(page, '#back')).eff > 0.9 && (await vis(page, '#dLabel')).text === 'bravo' && (await vis(page, '#detail')).eff > 0.9, J({ b: await vis(page, '#back'), l: await vis(page, '#dLabel') }));
  const hit = await lastSurface(page);
  check('X5 the hit stays detail while the card is ended', J(hit) === J({ window: 'open', hit: 'detail' }), J(hit));

  await setReply(page, card({ status: 'working' }));
  await fire(page, 'sessions', SUMMARY);
  await sleep(250);
  const endedAfter = await vis(page, '#dEnded');
  check('X6 a later non-null reply shows the rows again', (!endedAfter || endedAfter.eff < 0.01) && (await fields(page)).length === 4, J({ endedAfter, f: await fields(page) }));

  await page.locator('#back').click({ timeout: 2000 });
  await waitTransitionsDone(page, 1500);
  await sleep(300);
  const s = await vis(page, '#sess'), d = await vis(page, '#detail');
  check('X7 back shows the list and hides the card', s.eff > 0.9 && d.eff < 0.01, J({ s, d }));
  check('X8 back sends hit open', J(await lastSurface(page)) === J({ window: 'open', hit: 'open' }), J(await lastSurface(page)));
  const clip = parseClip(await clipOf(page));
  check('X9 back springs the clip to the open shape (420 x 342 for these rows)', clip && near(clip.w, 420, 0.6) && near(clip.h, 342, 0.6), J(clip));
  await page.close();
}

async function scenarioRecordsOnList() {
  const page = await openPage({ tag: 'records-list' });
  await fire(page, 'sessions', SUMMARY);
  await expand(page);
  await fire(page, 'records', undefined);
  await sleep(250);
  check('K1 a records event while the list shows calls nothing', (await invokes(page, 'session_detail')).length === 0, J(await invokes(page, 'session_detail')));
  await fire(page, 'records', null);
  await sleep(100);
  check('K2 ... also with a null payload', (await invokes(page, 'session_detail')).length === 0, J(await invokes(page, 'session_detail')));
  await page.close();
}

async function scenarioCollapse() {
  const page = await openPage({ tag: 'collapse' });
  await fire(page, 'sessions', SUMMARY);
  await expand(page);
  await openCard(page);
  check('C1 collapsing from the card reaches collapsed', await collapse(page), 'phase never became collapsed');
  await sleep(300);
  check('C2 the list shows again once collapsed', !/\boff\b/.test((await vis(page, '#sess')).cls) && /\boff\b/.test((await vis(page, '#detail')).cls), J({ s: await vis(page, '#sess'), d: await vis(page, '#detail') }));
  check('C3 the next expand starts on the list with hit open', await expand(page), 'no expand');
  await sleep(200);
  const s = await vis(page, '#sess'), d = await vis(page, '#detail');
  check('C4 list visible and card hidden after the next expand', s.eff > 0.9 && d.eff < 0.01, J({ s, d }));
  const hit = await lastSurface(page);
  check('C5 the next expand sends hit open, not detail', J(hit) === J({ window: 'open', hit: 'open' }), J(hit));
  await page.close();
}

async function scenarioNull() {
  const page = await openPage({ tag: 'null' });
  await fire(page, 'sessions', SUMMARY);
  await expand(page);
  await setReply(page, null);
  await page.locator('#rows button.srow').nth(1).click({ timeout: 2000 });
  await sleep(500);
  const s = await vis(page, '#sess'), d = await vis(page, '#detail');
  check('N1 a null reply to the first click leaves the list', s.eff > 0.9 && d.eff < 0.01, J({ s, d }));
  const calls = await invokes(page, 'session_detail');
  check('N2 the click did call session_detail', calls.length === 1 && J(calls[0]) === J({ id: 'id-two' }), J(calls));
  check('N3 the hit stays open', J(await lastSurface(page)) === J({ window: 'open', hit: 'open' }), J(await lastSurface(page)));
  const moreIsInert = await page.evaluate(() => document.querySelector('#rows div.srow.more') !== null);
  check('N4 the "more" row is present and clicking it calls nothing', moreIsInert && await (async () => {
    const before = (await invokes(page, 'session_detail')).length;
    await page.locator('#rows div.srow.more').click({ timeout: 2000 });
    await sleep(200);
    return (await invokes(page, 'session_detail')).length === before;
  })(), 'the more row called session_detail');
  await page.close();
}

async function scenarioReduced() {
  const page = await openPage({ tag: 'reduced', reduced: true });
  await fire(page, 'sessions', SUMMARY);
  await expand(page);
  await openCard(page);
  const clip = parseClip(await clipOf(page));
  check('M1 reduced motion: the card shows with the detail clip', (await vis(page, '#detail')).eff > 0.9 && clip && near(clip.h, 380, 0.6), J(clip));
  await page.close();
}

for (const s of [scenarioRows, scenarioOpen, scenarioFields, scenarioRefetch, scenarioRecordsOnList, scenarioCollapse, scenarioNull, scenarioReduced]) {
  try { await s(); } catch (e) { check(`${s.name} ran`, false, e.stack || String(e)); }
}
await browser.close();
check('E0 no page errors or console errors', pageErrors.length === 0, J(pageErrors));

let failed = 0;
for (const r of results) {
  if (r.ok) console.log(`ok   ${r.name}`);
  else { failed += 1; console.log(`FAIL ${r.name}: ${r.detail}`); }
}
console.log(failed ? `${failed} of ${results.length} detail checks failed` : `all ${results.length} detail checks passed`);
process.exit(failed ? 1 : 0);

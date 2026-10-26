// Loads dist/index.html in headless Chromium with a fake Tauri bridge and checks CONTRACT.md section 11, "Opening the
// session's app (amendment, 2026-10-26)", "Page": for a session with reach host_only the box is hidden and the
// `Open <app>` button (#openApp) takes its place (388 x 34 at (16, 316), radius 10, #1c1c1c, 600 12.5 px #f5f5f5), its
// label from the detail's `app` or else the host, the hint line, a real click calling open_host({id}) once and never
// focus, DOM focus staying put and no typing, each reply's hint and colour (opened, not_running, rejected, malformed),
// no second call while one is in flight, a reply after Back changing nothing, a redraw keeping or clearing the result
// hint, a redraw to reach inbox swapping the button for the box, a null detail hiding the button, and reach none and
// inbox keeping it hidden. Every expected value is a literal written from the contract text. Real time, no retries.
//
// Env: PLAYWRIGHT_CORE, CHROMIUM, NOTCH_PAGE, PAGE_CPU_THROTTLE as in tests/page_test.sh. Exit 0 when every check
// passes, 1 when one fails, 2 when the browser cannot start.
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

// Contract numbers (section 8): as tests/page/prompt-check.mjs.
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
const MUTED = 'rgb(139, 139, 139)', WHITE = 'rgb(245, 245, 245)', AMBER = 'rgb(245, 166, 35)';
const HOST = 'com.apple.TextEdit';
const INITIAL = 'It has no inbox, so notch-hud can only bring TextEdit forward.';

const results = [];
const check = (name, ok, detail) => results.push({ name, ok: !!ok, detail: ok ? undefined : detail });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const J = (v) => JSON.stringify(v);
const pageErrors = [];

// Installed before any page script: the fake bridge. `session_detail` answers with window.__reply(id); `focus` with
// window.__focusReply(args); `open_host` with window.__hostReply(args) (opened TextEdit by default).
function installBridge({ layout, windows }) {
  const handlers = {};
  const rec = (window.__rec = { invokes: [] });
  window.__reply = () => null;
  window.__focusReply = (a) => Promise.resolve(!!(a && a.on));
  window.__hostReply = () => Promise.resolve({ kind: 'opened', name: 'TextEdit' });
  window.__fire = (name, payload) => { (handlers[name] || []).forEach((h) => h({ payload })); };
  window.__TAURI__ = {
    event: { listen: (name, h) => { (handlers[name] = handlers[name] || []).push(h); return Promise.resolve(() => {}); } },
    core: {
      invoke: (cmd, args) => {
        rec.invokes.push({ cmd, args: args ? JSON.parse(JSON.stringify(args)) : null });
        if (cmd === 'layout') return Promise.resolve(layout);
        if (cmd === 'session_detail') return Promise.resolve(window.__reply(args && args.id));
        if (cmd === 'focus') return window.__focusReply(args);
        if (cmd === 'open_host') return window.__hostReply(args);
        if (cmd === 'surface') {
          const w = args && windows[args.window];
          return new Promise((resolve) => setTimeout(async () => { if (w) await window.__setWindow(w.w, w.h); resolve(null); }, 0));
        }
        return Promise.resolve(null);
      },
    },
  };
}

async function openPage(opts = {}) {
  const page = await browser.newPage({ viewport: { width: WINDOW.pill.w, height: WINDOW.pill.h } });
  const tag = opts.tag || 'page';
  page.on('pageerror', (e) => pageErrors.push(`${tag}: pageerror: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error') pageErrors.push(`${tag}: console: ${m.text()}`); });
  if (process.env.PAGE_CPU_THROTTLE) await (await page.context().newCDPSession(page)).send('Emulation.setCPUThrottlingRate', { rate: Number(process.env.PAGE_CPU_THROTTLE) });
  await page.exposeFunction('__setWindow', (w, h) => page.setViewportSize({ width: w, height: h }).catch(() => {}));
  await page.addInitScript(installBridge, { layout: LAYOUT, windows: WINDOW });
  await page.goto('file://' + PAGE_FILE);
  await page.waitForFunction(() => document.body && document.body.dataset.phase, null, { timeout: 1500 }).catch(() => {});
  return page;
}

const fire = (page, name, payload) => page.evaluate(([n, p]) => window.__fire(n, p), [name, payload]);
const waitPhase = (page, phase, ms) => page.waitForFunction((p) => document.body.dataset.phase === p, phase, { timeout: ms, polling: 20 }).then(() => true, () => false);
const waitTransitionsDone = (page, ms) => page.waitForFunction(() => document.getAnimations().every((a) => !(a instanceof CSSTransition) || a.playState !== 'running'), null, { timeout: ms, polling: 20 }).then(() => true, () => false);
const invokes = (page, cmd) => page.evaluate((c) => window.__rec.invokes.filter((i) => i.cmd === c).map((i) => i.args), cmd);
const setReply = (page, detail) => page.evaluate((d) => { window.__reply = () => d; }, detail);
const nowSec = () => Math.floor(Date.now() / 1000);

const row = (id, label, status, detail = null) => ({ id, label, status, detail });
const SUMMARY = { rows: [row('id-one', 'alpha', 'needs_permission', 'Bash'), row('id-two', 'bravo', 'working')], total: 2, attention: 1, working: 1 };
const card = (over = {}) => ({
  id: 'id-two', label: 'bravo', cwd: '/Users/lem/dev/bravo', status: 'working', event: 'PreToolUse',
  event_ts: nowSec() - 4, tool_name: 'Bash', agents: 0, reach: { kind: 'host_only', host: HOST }, app: 'TextEdit', ...over,
});

// What the button, the box and the hint line show.
const look = (page) => page.evaluate(() => {
  const shown = (e) => !!e && !e.hidden && getComputedStyle(e).display !== 'none' && e.getBoundingClientRect().height > 0;
  const b = document.querySelector('#openApp'), x = document.querySelector('#pbox'), h = document.querySelector('#phint');
  const c = document.querySelector('#content').getBoundingClientRect();
  const r = b ? b.getBoundingClientRect() : null, st = b ? getComputedStyle(b) : null;
  const a = document.activeElement;
  return {
    btnShown: shown(b), label: b ? b.textContent : null, boxShown: shown(x), hintShown: shown(h),
    hint: h ? h.textContent : null, hintColor: h ? getComputedStyle(h).color : null,
    geo: r ? { x: r.left - c.left, y: r.top - c.top, w: r.width, h: r.height } : null,
    radius: st && st.borderRadius, bg: st && st.backgroundColor, color: st && st.color, weight: st && st.fontWeight, size: st && st.fontSize,
    typing: !!x && x.classList.contains('typing'), active: a ? (a.id || a.tagName) : null,
    msgDisabled: document.querySelector('#msg') ? document.querySelector('#msg').disabled : null,
  };
});

// Hover in, open the card on row 2 with a real click, and wait for the spring.
async function openCard(page, reply = card()) {
  await fire(page, 'sessions', SUMMARY);
  await fire(page, 'hover', true);
  const ok = await waitPhase(page, 'expanded', 2500);
  await waitTransitionsDone(page, 1500);
  await setReply(page, reply);
  await showCard(page);
  return ok;
}
async function showCard(page) {
  await page.locator('#rows button.srow').nth(1).click({ timeout: 2000 });
  await page.waitForFunction(() => { const d = document.querySelector('#detail'); return d && !d.classList.contains('off'); }, null, { timeout: 2000, polling: 20 }).catch(() => {});
  await waitTransitionsDone(page, 1500);
  await sleep(700); // the 366 ms clip spring and the 120 ms crossfade
}
const clickButton = async (page) => { await page.locator('#openApp').click({ timeout: 2000 }); await sleep(150); };

// Scenarios ------------------------------------------------------------------------------------------------------

async function scenarioLook() {
  const page = await openPage({ tag: 'look' });
  check('B0 opens the card', await openCard(page), 'phase never became expanded');
  const l = await look(page);
  check('B1 host_only: #pbox is hidden and #openApp is shown', l.btnShown && !l.boxShown, J(l));
  check('B2 the button is 388 x 34 at (16, 316) of #content, radius 10', J(l.geo) === J({ x: 16, y: 316, w: 388, h: 34 }) && l.radius === '10px', J(l));
  check('B3 background #1c1c1c, text #f5f5f5, weight 600, 12.5 px', l.bg === 'rgb(28, 28, 28)' && l.color === WHITE && l.weight === '600' && l.size === '12.5px', J(l));
  check('B4 the label is "Open TextEdit" for app "TextEdit"', l.label === 'Open TextEdit', J(l));
  check('B5 the initial hint reads the bring-forward line in muted', l.hintShown && l.hint === INITIAL && l.hintColor === MUTED, J(l));
  check('B6 nothing called focus or open_host yet', (await invokes(page, 'focus')).length === 0 && (await invokes(page, 'open_host')).length === 0, J(await invokes(page, 'focus')));
  await page.close();

  for (const [name, app, label] of [['null', null, 'Open com.apple.TextEdit'], ['empty', '', 'Open com.apple.TextEdit'], ['missing', undefined, 'Open com.apple.TextEdit']]) {
    const p = await openPage({ tag: `label-${name}` });
    const reply = card({ app });
    if (app === undefined) delete reply.app;
    await openCard(p, reply);
    const m = await look(p);
    check(`B7 app ${name}: the label is "${label}" and the hint names the host`, m.btnShown && m.label === label && m.hint === 'It has no inbox, so notch-hud can only bring com.apple.TextEdit forward.', J(m));
    await p.close();
  }
}

async function scenarioClick() {
  const page = await openPage({ tag: 'click' });
  await openCard(page);
  const before = await look(page);
  await clickButton(page);
  const after = await look(page);
  check('C1 a real click calls open_host once with { id: "id-two" }', J(await invokes(page, 'open_host')) === J([{ id: 'id-two' }]), J(await invokes(page, 'open_host')));
  check('C2 it never calls focus, starts no typing, and DOM focus stays where it was (not the button, not #msg)', (await invokes(page, 'focus')).length === 0 && !after.typing && after.active === before.active && after.active !== 'openApp' && after.active !== 'msg', J({ before, after, f: await invokes(page, 'focus') }));
  check('C3 the reply { kind: "opened", name: "TextEdit" } sets "Brought TextEdit forward." in #f5f5f5', after.hint === 'Brought TextEdit forward.' && after.hintColor === WHITE, J(after));
  await page.close();

  for (const [name, reply, hint] of [
    ['not_running', '() => Promise.resolve({ kind: "not_running" })', 'TextEdit isn\'t running.'],
    ['rejected', '() => Promise.reject("no app to open for this session")', 'Couldn\'t bring TextEdit forward.'],
    ['null', '() => Promise.resolve(null)', 'Couldn\'t bring TextEdit forward.'],
    ['empty object', '() => Promise.resolve({})', 'Couldn\'t bring TextEdit forward.'],
  ]) {
    const p = await openPage({ tag: `reply-${name}` });
    await openCard(p);
    await p.evaluate((src) => { window.__hostReply = (0, eval)(src); }, reply);
    await clickButton(p);
    const l = await look(p);
    check(`C4 reply ${name}: the hint reads "${hint}" in amber`, l.hint === hint && l.hintColor === AMBER && l.btnShown, J(l));
    await p.close();
  }

  // a result hint uses the app's name from the reply for opened
  const named = await openPage({ tag: 'reply-name' });
  await openCard(named);
  await named.evaluate(() => { window.__hostReply = () => Promise.resolve({ kind: 'opened', name: 'Text Edit' }); });
  await clickButton(named);
  check('C5 opened uses the name in the reply', (await look(named)).hint === 'Brought Text Edit forward.', J(await look(named)));
  await named.close();
}

async function scenarioInFlight() {
  const page = await openPage({ tag: 'in-flight' });
  await openCard(page);
  await page.evaluate(() => { window.__hostReply = () => new Promise((r) => { window.__releaseHost = r; }); });
  await clickButton(page);
  await clickButton(page);
  check('P1 a second click while the first call is pending sends no second open_host', (await invokes(page, 'open_host')).length === 1, J(await invokes(page, 'open_host')));
  await page.evaluate(() => window.__releaseHost({ kind: 'opened', name: 'TextEdit' }));
  await sleep(150);
  check('P2 the reply then sets the hint', (await look(page)).hint === 'Brought TextEdit forward.', J(await look(page)));
  await clickButton(page);
  check('P3 once the call has resolved a click calls open_host again', (await invokes(page, 'open_host')).length === 2, J(await invokes(page, 'open_host')));
  await page.close();

  // a reply that arrives after Back closed the card changes nothing
  const late = await openPage({ tag: 'late-back' });
  await openCard(late);
  await late.evaluate(() => { window.__hostReply = () => new Promise((r) => { window.__releaseHost = r; }); });
  await clickButton(late);
  await late.locator('#back').click({ timeout: 2000 });
  await sleep(500);
  await late.evaluate(() => window.__releaseHost({ kind: 'opened', name: 'TextEdit' }));
  await sleep(150);
  await showCard(late);
  const l = await look(late);
  check('P4 a reply that arrives after Back changes nothing: the reopened card shows the initial hint', l.btnShown && l.hint === INITIAL && l.hintColor === MUTED, J(l));
  await late.close();
}

async function scenarioRedraw() {
  const page = await openPage({ tag: 'redraw' });
  await openCard(page);
  await clickButton(page);
  await fire(page, 'records', null);
  await sleep(300);
  let l = await look(page);
  check('D1 a records redraw with the same reach keeps the result hint', l.hint === 'Brought TextEdit forward.' && l.hintColor === WHITE && l.btnShown, J(l));

  await setReply(page, card({ reach: { kind: 'host_only', host: 'com.apple.Notes' }, app: 'Notes' }));
  await fire(page, 'records', null);
  await sleep(300);
  l = await look(page);
  check('D2 a redraw with another host clears it: the button and hint name the new app', l.btnShown && l.label === 'Open Notes' && l.hint === 'It has no inbox, so notch-hud can only bring Notes forward.' && l.hintColor === MUTED, J(l));

  await clickButton(page);
  await setReply(page, card());
  await fire(page, 'records', null);
  await sleep(300);
  l = await look(page);
  check('D3 a redraw back to the first host clears the result too', l.hint === INITIAL && l.label === 'Open TextEdit', J(l));

  await clickButton(page);
  await setReply(page, card({ reach: { kind: 'inbox' }, app: null }));
  await fire(page, 'records', null);
  await sleep(300);
  l = await look(page);
  check('D4 a redraw to reach inbox shows the box, hides the button and clears the result', l.boxShown && !l.btnShown && l.msgDisabled === false && l.hint === '', J(l));

  await setReply(page, card());
  await fire(page, 'records', null);
  await sleep(300);
  l = await look(page);
  check('D5 a redraw back to host_only hides the box and shows the button with the initial hint', !l.boxShown && l.btnShown && l.hint === INITIAL, J(l));
  await page.close();

  const ended = await openPage({ tag: 'ended' });
  await openCard(ended);
  await setReply(ended, null);
  await fire(ended, 'records', null);
  await sleep(300);
  const e = await look(ended);
  check('D6 a null detail reply (Session ended) hides the button', !e.btnShown, J(e));
  await ended.close();
}

async function scenarioOtherReach() {
  for (const [name, reach, app] of [['none', { kind: 'none' }, null], ['inbox', { kind: 'inbox' }, null]]) {
    const page = await openPage({ tag: `reach-${name}` });
    await openCard(page, card({ reach, app }));
    const l = await look(page);
    check(`N1 reach ${name}: the button is hidden and the box is shown`, !l.btnShown && l.boxShown, J(l));
    await page.close();
  }
}

for (const s of [scenarioLook, scenarioClick, scenarioInFlight, scenarioRedraw, scenarioOtherReach]) {
  try { await s(); } catch (e) { check(`${s.name} ran`, false, e.stack || String(e)); }
}
await browser.close();
check('E0 no page errors or console errors', pageErrors.length === 0, J(pageErrors));

let failed = 0;
for (const r of results) {
  if (r.ok) console.log(`ok   ${r.name}`);
  else { failed += 1; console.log(`FAIL ${r.name}: ${r.detail}`); }
}
console.log(failed ? `${failed} of ${results.length} open-host checks failed` : `all ${results.length} open-host checks passed`);
process.exit(failed ? 1 : 0);

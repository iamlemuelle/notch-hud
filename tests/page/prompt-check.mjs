// Loads dist/index.html in headless Chromium with a fake Tauri bridge and checks CONTRACT.md section 11 "Prompt box"
// (typing only) and section 8 "Typing (amendment)": a click on the box calls focus(true) and enters typing, the hint
// line, hover-out during typing keeps the panel open, Esc, a `key` false event and leaving the card call focus(false)
// and end typing, the panel then collapses when the pointer is gone, Return calls prompt_draft with the draft and
// nothing else, a focus(true) that does not take ends typing, and keys of an input method composition do nothing. Every
// expected value is a literal written from the contract text. Real time, no retries.
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

// Contract numbers (section 8): as tests/page/detail-check.mjs.
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
const HINT = 'Return to send, Shift+Return for a new line, Esc to stop typing';

const results = [];
const check = (name, ok, detail) => results.push({ name, ok: !!ok, detail: ok ? undefined : detail });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const J = (v) => JSON.stringify(v);
const pageErrors = [];

// Installed before any page script: the fake bridge. `session_detail` answers with window.__reply(id); `focus` with
// window.__focusReply(args), by default at once with whether the panel is key afterwards: `on`.
function installBridge({ layout, windows }) {
  const handlers = {};
  const rec = (window.__rec = { invokes: [] });
  window.__reply = () => null;
  window.__focusReply = (a) => Promise.resolve(!!(a && a.on));
  window.__fire = (name, payload) => { (handlers[name] || []).forEach((h) => h({ payload })); };
  window.__TAURI__ = {
    event: { listen: (name, h) => { (handlers[name] = handlers[name] || []).push(h); return Promise.resolve(() => {}); } },
    core: {
      invoke: (cmd, args) => {
        rec.invokes.push({ cmd, args: args ? JSON.parse(JSON.stringify(args)) : null });
        if (cmd === 'layout') return Promise.resolve(layout);
        if (cmd === 'session_detail') return Promise.resolve(window.__reply(args && args.id));
        if (cmd === 'focus') return window.__focusReply(args);
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
  event_ts: nowSec() - 4, tool_name: 'Bash', agents: 0, ...over,
});

// What the box shows: the typing ring, the hint text, the send button, the draft, DOM focus and visibility.
const box = (page) => page.evaluate(() => {
  const b = document.querySelector('#pbox'), m = document.querySelector('#msg'), h = document.querySelector('#phint'), s = document.querySelector('#send');
  const shown = (e) => !!e && !e.hidden && getComputedStyle(e).display !== 'none' && e.getBoundingClientRect().height > 0;
  return {
    typing: !!b && b.classList.contains('typing'), hint: h ? h.textContent : null, send: s ? !s.disabled : null,
    value: m ? m.value : null, focused: document.activeElement === m, placeholder: m ? m.placeholder : null,
    boxShown: shown(b), hintShown: shown(h),
  };
});

// Hover in, open the card on row 2 with a real click, and wait for the spring.
async function openCard(page, reply = card()) {
  await fire(page, 'sessions', SUMMARY);
  await fire(page, 'hover', true);
  const ok = await waitPhase(page, 'expanded', 2500);
  await waitTransitionsDone(page, 1500);
  await setReply(page, reply);
  await page.locator('#rows button.srow').nth(1).click({ timeout: 2000 });
  await page.waitForFunction(() => { const d = document.querySelector('#detail'); return d && !d.classList.contains('off'); }, null, { timeout: 2000, polling: 20 }).catch(() => {});
  await waitTransitionsDone(page, 1500);
  await sleep(700); // the 366 ms clip spring and the 120 ms crossfade
  return ok;
}
// A real click on the box's text area.
const clickBox = async (page) => { await page.locator('#msg').click({ timeout: 2000 }); await sleep(150); };

// Scenarios ------------------------------------------------------------------------------------------------------

async function scenarioLook() {
  const page = await openPage({ tag: 'look' });
  check('L0 opens the card', await openCard(page), 'phase never became expanded');
  const b = await box(page);
  check('L1 the card shows the box and an empty hint line, not typing', b.boxShown && b.hintShown && !b.typing && b.hint === '', J(b));
  check('L2 the placeholder is "Message bravo" and the send button is disabled while the box is empty', b.placeholder === 'Message bravo' && b.send === false, J(b));
  const geo = await page.evaluate(() => {
    const c = document.querySelector('#content').getBoundingClientRect();
    const r = (s) => { const e = document.querySelector(s).getBoundingClientRect(); return { x: e.left - c.left, y: e.top - c.top, w: e.width, h: e.height }; };
    const st = getComputedStyle(document.querySelector('#pbox'));
    return { box: r('#pbox'), hint: r('#phint'), send: r('#send'), radius: st.borderRadius, bg: st.backgroundColor, ta: getComputedStyle(document.querySelector('#msg')).font };
  });
  // Card top 196, header 18, 4 pt, four 22 pt rows, 10 pt: the box at 316, 34 tall; 6 pt; the hint at 356, 14 tall.
  check('L3 the box is 388 x 34 at (16, 316), radius 10, background #141414', J(geo.box) === J({ x: 16, y: 316, w: 388, h: 34 }) && geo.radius === '10px' && geo.bg === 'rgb(20, 20, 20)', J(geo));
  check('L4 the hint line is 388 x 14 at (16, 356)', J(geo.hint) === J({ x: 16, y: 356, w: 388, h: 14 }), J(geo));
  check('L5 the send button is 26 x 26, 4 pt from the box\'s right edge', geo.send.w === 26 && geo.send.h === 26 && geo.send.x + 26 === 16 + 388 - 4, J(geo));
  check('L6 nothing called focus before a click on the box', (await invokes(page, 'focus')).length === 0, J(await invokes(page, 'focus')));
  await page.close();
}

async function scenarioType() {
  const page = await openPage({ tag: 'type' });
  await openCard(page);
  await clickBox(page);
  const calls = await invokes(page, 'focus');
  check('T1 a click on the box calls focus({on: true}) once', J(calls) === J([{ on: true }]), J(calls));
  const b = await box(page);
  check('T2 typing: the ring is on, the hint reads the typing line, the box has DOM focus', b.typing && b.hint === HINT && b.focused, J(b));

  await page.keyboard.type('hello');
  await page.keyboard.press('Enter');
  await sleep(100);
  const drafts = await invokes(page, 'prompt_draft');
  check('T3 Return calls prompt_draft({text: "hello"}) once', J(drafts) === J([{ text: 'hello' }]), J(drafts));
  const after = await box(page);
  check('T4 Return keeps typing and the draft, and adds no newline', after.typing && after.value === 'hello' && after.hint === HINT, J(after));
  check('T5 Return sends nothing: no command other than prompt_draft, focus, surface and session_detail', J((await page.evaluate(() => window.__rec.invokes.map((i) => i.cmd))).filter((c) => !['layout', 'media', 'sessions', 'avatar', 'mood', 'surface', 'session_detail', 'focus', 'prompt_draft'].includes(c))) === '[]', J(await page.evaluate(() => window.__rec.invokes.map((i) => i.cmd))));

  await page.keyboard.press('Shift+Enter');
  await page.keyboard.type('x');
  await sleep(100);
  check('T6 Shift+Return adds a newline and calls nothing', (await box(page)).value === 'hello\nx' && (await invokes(page, 'prompt_draft')).length === 1, J({ b: await box(page), d: await invokes(page, 'prompt_draft') }));
  await page.keyboard.press('Enter');
  await sleep(100);
  check('T7 Return sends the whole draft, newline included', J((await invokes(page, 'prompt_draft')).pop()) === J({ text: 'hello\nx' }), J(await invokes(page, 'prompt_draft')));

  await page.evaluate(() => { const m = document.querySelector('#msg'); m.value = '   \n  '; m.dispatchEvent(new Event('input')); });
  const n = (await invokes(page, 'prompt_draft')).length;
  await page.keyboard.press('Enter');
  await sleep(100);
  check('T8 Return with only whitespace calls nothing, and the send button is disabled', (await invokes(page, 'prompt_draft')).length === n && (await box(page)).send === false, J({ d: await invokes(page, 'prompt_draft'), b: await box(page) }));

  await page.evaluate(() => { const m = document.querySelector('#msg'); m.value = 'sent by button'; m.dispatchEvent(new Event('input')); });
  check('T9 the send button is enabled with a draft', (await box(page)).send === true, J(await box(page)));
  await page.locator('#send').click({ timeout: 2000 });
  await sleep(100);
  const b9 = await box(page);
  check('T10 the send button calls prompt_draft with the draft and typing goes on', J((await invokes(page, 'prompt_draft')).pop()) === J({ text: 'sent by button' }) && b9.typing && b9.focused, J({ d: await invokes(page, 'prompt_draft'), b9 }));
  check('T11 typing called focus(true) once in all', J(await invokes(page, 'focus')) === J([{ on: true }]), J(await invokes(page, 'focus')));
  await page.close();
}

async function scenarioPadding() {
  const page = await openPage({ tag: 'padding' });
  await openCard(page);
  const r = await page.evaluate(() => { const e = document.querySelector('#pbox').getBoundingClientRect(); return { x: e.left, y: e.top + e.height / 2 }; });
  await page.mouse.click(r.x + 4, r.y);
  await sleep(150);
  const b = await box(page);
  check('T12 a click on the box\'s left padding starts typing and, once focus returns, puts the caret in the box', b.typing && b.focused && J(await invokes(page, 'focus')) === J([{ on: true }]), J({ b, f: await invokes(page, 'focus') }));
  await page.close();
}

async function scenarioHoverOut() {
  const page = await openPage({ tag: 'hover-out' });
  await openCard(page);
  await clickBox(page);
  const surfaces = (await invokes(page, 'surface')).length;
  await fire(page, 'hover', false);
  await sleep(800);
  check('H1 hover-out while typing does not collapse: still expanded 800 ms later', await page.evaluate(() => document.body.dataset.phase) === 'expanded' && (await box(page)).typing, J({ phase: await page.evaluate(() => document.body.dataset.phase), b: await box(page) }));
  check('H2 ... and sends no new surface call', (await invokes(page, 'surface')).length === surfaces, J((await invokes(page, 'surface')).slice(surfaces)));
  await page.keyboard.press('Escape');
  await sleep(100);
  const b = await box(page);
  check('H3 Esc calls focus({on: false}) and ends typing: no ring, an empty hint, no DOM focus', J(await invokes(page, 'focus')) === J([{ on: true }, { on: false }]) && !b.typing && b.hint === '' && !b.focused, J({ f: await invokes(page, 'focus'), b }));
  check('H5 with the pointer gone, the panel then collapses as usual', await waitPhase(page, 'collapsed', 2000), await page.evaluate(() => document.body.dataset.phase));
  check('H6 collapsing after typing calls focus(false) no second time', J(await invokes(page, 'focus')) === J([{ on: true }, { on: false }]), J(await invokes(page, 'focus')));
  await page.close();

  // A press on the box inside the 250 ms after a hover out cancels that pending collapse. Both happen in one task, so
  // no round trip of the test falls inside the grace.
  const page2 = await openPage({ tag: 'hover-out-pending' });
  await openCard(page2);
  await page2.evaluate(() => {
    window.__fire('hover', false);
    document.querySelector('#pbox').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true }));
  });
  await sleep(700);
  check('H7 typing started inside the hover-out grace cancels the pending collapse', await page2.evaluate(() => document.body.dataset.phase) === 'expanded' && (await box(page2)).typing, J({ phase: await page2.evaluate(() => document.body.dataset.phase), b: await box(page2) }));
  await page2.close();
}

async function scenarioIme() {
  const page = await openPage({ tag: 'ime' });
  await openCard(page);
  await clickBox(page);
  await page.keyboard.type('nihon');
  // WebKit: the keydown that commits a candidate has isComposing false and keyCode 229; while composing, isComposing.
  const key = (init, keyCode) => page.evaluate(([i, k]) => {
    const e = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...i });
    if (k !== undefined) Object.defineProperty(e, 'keyCode', { get: () => k });
    document.querySelector('#msg').dispatchEvent(e);
  }, [init, keyCode]);
  await key({ key: 'Enter', isComposing: true });
  await key({ key: 'Enter' }, 229);
  await key({ key: 'Escape', isComposing: true });
  await key({ key: 'Escape' }, 229);
  await sleep(100);
  check('I1 Return and Esc that belong to an input method composition do nothing: no draft, typing goes on', (await invokes(page, 'prompt_draft')).length === 0 && (await box(page)).typing && J(await invokes(page, 'focus')) === J([{ on: true }]), J({ d: await invokes(page, 'prompt_draft'), b: await box(page), f: await invokes(page, 'focus') }));
  await page.close();
}

async function scenarioEscStays() {
  const page = await openPage({ tag: 'esc-hovered' });
  await openCard(page);
  await clickBox(page);
  await page.keyboard.type('keep me');
  await page.keyboard.press('Escape');
  await sleep(800);
  const b = await box(page);
  check('E1 Esc with the pointer still on the panel: typing ends and the panel stays expanded', !b.typing && await page.evaluate(() => document.body.dataset.phase) === 'expanded', J({ b, phase: await page.evaluate(() => document.body.dataset.phase) }));
  check('E2 Esc keeps the draft', b.value === 'keep me', J(b));
  await page.keyboard.press('Enter');
  await sleep(100);
  check('E3 Return after Esc calls nothing: the keyboard is no longer the box\'s', (await invokes(page, 'prompt_draft')).length === 0, J(await invokes(page, 'prompt_draft')));
  await clickBox(page);
  check('E4 a second click on the box types again: focus(true) once more', J(await invokes(page, 'focus')) === J([{ on: true }, { on: false }, { on: true }]) && (await box(page)).typing, J(await invokes(page, 'focus')));
  await page.close();
}

async function scenarioKeyFalse() {
  const page = await openPage({ tag: 'key-false' });
  await openCard(page);
  await clickBox(page);
  await fire(page, 'key', false);
  await sleep(100);
  const b = await box(page);
  check('K1 a key false event ends typing and calls focus({on: false})', !b.typing && b.hint === '' && J(await invokes(page, 'focus')) === J([{ on: true }, { on: false }]), J({ b, f: await invokes(page, 'focus') }));
  await fire(page, 'key', false);
  await sleep(100);
  check('K2 a second key false event is a no-op', J(await invokes(page, 'focus')) === J([{ on: true }, { on: false }]), J(await invokes(page, 'focus')));
  await sleep(500);
  check('K3 with the pointer still on the panel it stays expanded', await page.evaluate(() => document.body.dataset.phase) === 'expanded', await page.evaluate(() => document.body.dataset.phase));
  await clickBox(page);
  await fire(page, 'hover', false);
  await sleep(100);
  await fire(page, 'key', false);
  check('K4 key false after the pointer left: the panel collapses as usual', await waitPhase(page, 'collapsed', 2000), await page.evaluate(() => document.body.dataset.phase));

  // A key false that arrives while focus(true) has not returned is the previous give-back's: it is ignored.
  const page2 = await openPage({ tag: 'key-false-pending' });
  await openCard(page2);
  await page2.evaluate(() => { window.__focusReply = (a) => a.on ? new Promise((r) => { window.__releaseFocus = r; }) : Promise.resolve(null); });
  await clickBox(page2);
  await fire(page2, 'key', false);
  await sleep(100);
  check('K5 key false while focus(true) is pending does not end typing', (await box(page2)).typing && J(await invokes(page2, 'focus')) === J([{ on: true }]), J({ b: await box(page2), f: await invokes(page2, 'focus') }));
  await page2.evaluate(() => window.__releaseFocus(true));
  await sleep(100);
  check('K6 once focus(true) returns the box has DOM focus', (await box(page2)).focused && (await box(page2)).typing, J(await box(page2)));
  await fire(page2, 'key', false);
  await sleep(100);
  check('K7 a key false after it returns ends typing', !(await box(page2)).typing, J(await box(page2)));
  await page.close();
  await page2.close();

  // focus(true) that leaves the panel not key (false), or fails: no keys and no `key` event can come, so typing ends.
  // The reply is held until the pointer has left, so the hover-out grace never races the click.
  for (const [how, reply] of [['returns false', 'false'], ['rejects', 'reject']]) {
    const p = await openPage({ tag: `focus-${reply}` });
    await openCard(p);
    await p.evaluate(() => { window.__focusReply = (a) => a.on ? new Promise((ok, no) => { window.__settleFocus = { ok, no }; }) : Promise.resolve(false); });
    await clickBox(p);
    await fire(p, 'hover', false);
    await p.evaluate((r) => r === 'false' ? window.__settleFocus.ok(false) : window.__settleFocus.no(new Error('no')), reply);
    await sleep(100);
    const b = await box(p);
    check(`K8 focus(true) ${how}: typing ends with no ring and an empty hint, and focus({on: false}) follows`, !b.typing && b.hint === '' && J(await invokes(p, 'focus')) === J([{ on: true }, { on: false }]), J({ b, f: await invokes(p, 'focus') }));
    check(`K9 focus(true) ${how} with the pointer gone: the panel collapses as usual`, await waitPhase(p, 'collapsed', 2000), await p.evaluate(() => document.body.dataset.phase));
    await p.close();
  }
}

async function scenarioLeaveCard() {
  const page = await openPage({ tag: 'leave-card' });
  await openCard(page);
  await clickBox(page);
  await page.keyboard.type('draft');
  await page.locator('#back').click({ timeout: 2000 });
  await sleep(300);
  check('C1 back while typing calls focus({on: false}) and ends typing', J(await invokes(page, 'focus')) === J([{ on: true }, { on: false }]) && !(await box(page)).typing, J({ f: await invokes(page, 'focus'), b: await box(page) }));
  check('C2 leaving the card clears the draft', (await box(page)).value === '', J(await box(page)));

  await page.locator('#rows button.srow').nth(1).click({ timeout: 2000 });
  await sleep(700);
  await clickBox(page);
  await page.locator('#btnNext').click({ timeout: 2000, force: true });
  await sleep(150);
  check('C3 a click elsewhere in the panel ends typing', !(await box(page)).typing && J(await invokes(page, 'focus')) === J([{ on: true }, { on: false }, { on: true }, { on: false }]), J({ f: await invokes(page, 'focus'), b: await box(page) }));

  await clickBox(page);
  await setReply(page, null);
  await fire(page, 'sessions', SUMMARY);
  await sleep(250);
  const b = await box(page);
  check('C4 the session ending while typing hides the box and the hint and calls focus({on: false})', !b.typing && !b.boxShown && !b.hintShown && (await invokes(page, 'focus')).pop().on === false, J({ b, f: await invokes(page, 'focus') }));
  await page.close();

  const page2 = await openPage({ tag: 'collapse-typing' });
  await openCard(page2);
  await clickBox(page2);
  await page2.keyboard.type('gone');
  await fire(page2, 'hover', false);
  await page2.keyboard.press('Escape');
  check('C5 after Esc with the pointer gone the panel collapses', await waitPhase(page2, 'collapsed', 2000), await page2.evaluate(() => document.body.dataset.phase));
  await sleep(200);
  const b2 = await box(page2);
  check('C6 once collapsed, the draft is cleared and the next expand starts on the list', b2.value === '' && J(await invokes(page2, 'focus')) === J([{ on: true }, { on: false }]), J({ b2, f: await invokes(page2, 'focus') }));
  await page2.close();
}

async function scenarioNoCard() {
  const page = await openPage({ tag: 'no-card' });
  await fire(page, 'sessions', SUMMARY);
  await fire(page, 'hover', true);
  await waitPhase(page, 'expanded', 2500);
  await sleep(500);
  await page.evaluate(() => document.querySelector('#pbox').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })));
  await sleep(100);
  check('N1 the box cannot start typing while the list shows', (await invokes(page, 'focus')).length === 0 && !(await box(page)).typing, J(await invokes(page, 'focus')));
  await page.keyboard.press('Escape');
  await page.keyboard.press('Enter');
  check('N2 Esc and Return outside typing call nothing', (await invokes(page, 'focus')).length === 0 && (await invokes(page, 'prompt_draft')).length === 0, J({ f: await invokes(page, 'focus'), d: await invokes(page, 'prompt_draft') }));
  await page.close();

  const page2 = await openPage({ tag: 'collapsing' });
  await openCard(page2);
  await fire(page2, 'hover', false);
  const collapsing = await waitPhase(page2, 'collapsing', 1500);
  await page2.evaluate(() => document.querySelector('#pbox').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })));
  await sleep(100);
  check('N3 a press on the box while collapsing does not start typing', collapsing && (await invokes(page2, 'focus')).length === 0 && !(await box(page2)).typing, J({ collapsing, f: await invokes(page2, 'focus') }));
  await page2.close();
}

for (const s of [scenarioLook, scenarioType, scenarioPadding, scenarioHoverOut, scenarioIme, scenarioEscStays, scenarioKeyFalse, scenarioLeaveCard, scenarioNoCard]) {
  try { await s(); } catch (e) { check(`${s.name} ran`, false, e.stack || String(e)); }
}
await browser.close();
check('E0 no page errors or console errors', pageErrors.length === 0, J(pageErrors));

let failed = 0;
for (const r of results) {
  if (r.ok) console.log(`ok   ${r.name}`);
  else { failed += 1; console.log(`FAIL ${r.name}: ${r.detail}`); }
}
console.log(failed ? `${failed} of ${results.length} prompt checks failed` : `all ${results.length} prompt checks passed`);
process.exit(failed ? 1 : 0);

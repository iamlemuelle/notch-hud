// Loads dist/index.html in headless Chromium with a fake Tauri bridge and checks CONTRACT.md section 11 "Prompt box"
// with its "Sending" amendment and section 8 "Typing (amendment)": a click on the box calls focus(true) and enters
// typing, the hint line, hover-out during typing keeps the panel open, Esc, a `key` false event and leaving the card
// call focus(false) and end typing, the panel then collapses when the pointer is gone, Return and the send button call
// prompt({id, text}) with the card's id and the trimmed draft and end typing, every delivery state (sending, sent,
// picked up by a records event or by status, no reply after 10 s, failed) with its hint and colour, the disabled box
// for a session with no inbox (reach none or host_only), a reach that changes away from inbox ending typing, a
// focus(true) that does not take ending typing, a held failure ending when the session ends or loses its inbox, and keys of an input method composition doing nothing. Every expected
// value is a literal written from the contract text. Real time, no retries.
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
const MUTED = 'rgb(139, 139, 139)', WHITE = 'rgb(245, 245, 245)', AMBER = 'rgb(245, 166, 35)';
const SENT = 'Sent. Waiting for bravo to pick it up.';
const NO_REPLY = 'No reply yet. It may be waiting for approval in that session.';
const NO_INBOX = 'It has no inbox notch-hud can send to.';
const HINT = 'Return to send, Shift+Return for a new line, Esc to stop typing';

const results = [];
const check = (name, ok, detail) => results.push({ name, ok: !!ok, detail: ok ? undefined : detail });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const J = (v) => JSON.stringify(v);
const pageErrors = [];

// Installed before any page script: the fake bridge. `session_detail` answers with window.__reply(id); `focus` with
// window.__focusReply(args), `prompt` with window.__promptReply(args) (resolve null by default), by default at once with whether the panel is key afterwards: `on`.
function installBridge({ layout, windows }) {
  const handlers = {};
  const rec = (window.__rec = { invokes: [] });
  window.__reply = () => null;
  window.__focusReply = (a) => Promise.resolve(!!(a && a.on));
  window.__promptReply = () => Promise.resolve(null);
  window.__fire = (name, payload) => { (handlers[name] || []).forEach((h) => h({ payload })); };
  window.__TAURI__ = {
    event: { listen: (name, h) => { (handlers[name] = handlers[name] || []).push(h); return Promise.resolve(() => {}); } },
    core: {
      invoke: (cmd, args) => {
        rec.invokes.push({ cmd, args: args ? JSON.parse(JSON.stringify(args)) : null });
        if (cmd === 'layout') return Promise.resolve(layout);
        if (cmd === 'session_detail') return Promise.resolve(window.__reply(args && args.id));
        if (cmd === 'focus') return window.__focusReply(args);
        if (cmd === 'prompt') return window.__promptReply(args);
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
  event_ts: nowSec() - 4, tool_name: 'Bash', agents: 0, reach: { kind: 'inbox' }, ...over,
});

// What the box shows: the typing ring, the hint text, the send button, the draft, DOM focus and visibility.
const box = (page) => page.evaluate(() => {
  const b = document.querySelector('#pbox'), m = document.querySelector('#msg'), h = document.querySelector('#phint'), s = document.querySelector('#send');
  const shown = (e) => !!e && !e.hidden && getComputedStyle(e).display !== 'none' && e.getBoundingClientRect().height > 0;
  return {
    typing: !!b && b.classList.contains('typing'), hint: h ? h.textContent : null, send: s ? !s.disabled : null,
    value: m ? m.value : null, focused: document.activeElement === m, placeholder: m ? m.placeholder : null,
    boxShown: shown(b), hintShown: shown(h),
    color: h ? getComputedStyle(h).color : null, disabled: m ? m.disabled : null,
    phColor: m ? getComputedStyle(m, '::placeholder').color : null,
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

const typeAndSend = async (page, text) => { await clickBox(page); await page.keyboard.type(text); await page.keyboard.press('Enter'); await sleep(100); };

async function scenarioType() {
  const page = await openPage({ tag: 'type' });
  await openCard(page);
  await clickBox(page);
  const calls = await invokes(page, 'focus');
  check('T1 a click on the box calls focus({on: true}) once', J(calls) === J([{ on: true }]), J(calls));
  const b = await box(page);
  check('T2 typing: the ring is on, the hint reads the typing line, the box has DOM focus', b.typing && b.hint === HINT && b.focused, J(b));

  await page.keyboard.type('  hello  ');
  await page.keyboard.press('Enter');
  await sleep(100);
  const sends = await invokes(page, 'prompt');
  check('T3 Return calls prompt({id: "id-two", text: "hello"}) once, with the draft trimmed', J(sends) === J([{ id: 'id-two', text: 'hello' }]), J(sends));
  const after = await box(page);
  check('T4 Return ends typing (focus({on: false})), clears the draft once sent and adds no newline', !after.typing && after.value === '' && J(await invokes(page, 'focus')) === J([{ on: true }, { on: false }]), J({ after, f: await invokes(page, 'focus') }));
  check('T4b the hint then reads the sent state', after.hint === SENT, J(after));
  const cmds = await page.evaluate(() => window.__rec.invokes.map((i) => i.cmd));
  check('T5 no command other than prompt, focus, surface and the page\'s own reads was called', J(cmds.filter((c) => !['layout', 'media', 'sessions', 'avatar', 'mood', 'surface', 'session_detail', 'focus', 'prompt'].includes(c))) === '[]', J(cmds));

  await clickBox(page);
  await page.keyboard.type('hello');
  await page.keyboard.press('Shift+Enter');
  await page.keyboard.type('x');
  await sleep(100);
  check('T6 Shift+Return adds a newline and sends nothing', (await box(page)).value === 'hello\nx' && (await box(page)).typing && (await invokes(page, 'prompt')).length === 1, J({ b: await box(page), d: await invokes(page, 'prompt') }));
  await page.keyboard.press('Enter');
  await sleep(100);
  check('T7 Return sends the whole draft, newline included', J((await invokes(page, 'prompt')).pop()) === J({ id: 'id-two', text: 'hello\nx' }), J(await invokes(page, 'prompt')));

  await clickBox(page);
  await page.evaluate(() => { const m = document.querySelector('#msg'); m.value = '   \n  '; m.dispatchEvent(new Event('input')); });
  const n = (await invokes(page, 'prompt')).length;
  await page.keyboard.press('Enter');
  await sleep(100);
  check('T8 Return with only whitespace calls nothing, typing goes on, and the send button is disabled', (await invokes(page, 'prompt')).length === n && (await box(page)).send === false && (await box(page)).typing, J({ d: await invokes(page, 'prompt'), b: await box(page) }));

  await page.evaluate(() => { const m = document.querySelector('#msg'); m.value = ' sent by button '; m.dispatchEvent(new Event('input')); });
  check('T9 the send button is enabled with a draft', (await box(page)).send === true, J(await box(page)));
  const focusBefore = (await invokes(page, 'focus')).length;
  await page.locator('#send').click({ timeout: 2000 });
  await sleep(100);
  const b9 = await box(page);
  check('T10 the send button calls prompt with the card\'s id and the trimmed draft and ends typing', J((await invokes(page, 'prompt')).pop()) === J({ id: 'id-two', text: 'sent by button' }) && !b9.typing && b9.value === '', J({ d: await invokes(page, 'prompt'), b9 }));
  check('T11 each send called focus({on: false}) once', J((await invokes(page, 'focus')).slice(focusBefore)) === J([{ on: false }]), J(await invokes(page, 'focus')));
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
  check('I1 Return and Esc that belong to an input method composition do nothing: nothing is sent, typing goes on', (await invokes(page, 'prompt')).length === 0 && (await box(page)).typing && J(await invokes(page, 'focus')) === J([{ on: true }]), J({ d: await invokes(page, 'prompt'), b: await box(page), f: await invokes(page, 'focus') }));
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
  check('E3 Return after Esc calls nothing: the keyboard is no longer the box\'s', (await invokes(page, 'prompt')).length === 0, J(await invokes(page, 'prompt')));
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
  check('N2 Esc and Return outside typing call nothing', (await invokes(page, 'focus')).length === 0 && (await invokes(page, 'prompt')).length === 0, J({ f: await invokes(page, 'focus'), d: await invokes(page, 'prompt') }));
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

// Holds the next prompt call until released; returns the page's settle functions.
const holdPrompt = (page) => page.evaluate(() => { window.__promptReply = () => new Promise((ok, no) => { window.__promptSettle = { ok, no }; }); });
const hintOf = async (page) => { const b = await box(page); return { hint: b.hint, color: b.color }; };

async function scenarioDelivery() {
  // sending, then sent, with the draft cleared
  const page = await openPage({ tag: 'delivery-sent' });
  await openCard(page, card({ status: 'done' }));
  await holdPrompt(page);
  await clickBox(page);
  await page.keyboard.type('hi there');
  await page.keyboard.press('Enter');
  await sleep(150);
  let b = await box(page);
  check('D1 while the call is in flight the hint reads "Sending…" in muted, the send button is disabled and the draft stays', b.hint === 'Sending\u2026' && b.color === MUTED && b.send === false && b.value === 'hi there' && !b.typing, J(b));
  await page.evaluate(() => window.__promptSettle.ok(null));
  await sleep(150);
  b = await box(page);
  check('D2 once the call resolves the hint reads "Sent. Waiting for bravo to pick it up." in muted and the draft is cleared', b.hint === SENT && b.color === MUTED && b.value === '', J(b));
  await sleep(5000);
  check('D3 5 s later, with no pickup, the hint still reads the sent state', (await box(page)).hint === SENT, J(await box(page)));
  await sleep(5600);
  b = await box(page);
  check('D4 10 s after sent with no pickup the hint reads "No reply yet. It may be waiting for approval in that session." in amber', b.hint === NO_REPLY && b.color === AMBER, J(b));
  await clickBox(page);
  b = await box(page);
  check('D5 typing again clears the no reply state: the hint reads the typing line', b.hint === HINT && b.color === MUTED, J(b));
  await page.close();

  // picked up: a records event whose session_detail reply has a new event_ts
  const p2 = await openPage({ tag: 'delivery-records' });
  const ts2 = nowSec() - 30;
  await openCard(p2, card({ status: 'done', event_ts: ts2 }));
  await typeAndSend(p2, 'go');
  check('D6 sent first', (await box(p2)).hint === SENT, J(await box(p2)));
  await setReply(p2, card({ status: 'done', event_ts: ts2 }));
  await fire(p2, 'records', null);
  await sleep(250);
  check('D7 a records event whose card has the same event_ts and status is not a pickup', (await box(p2)).hint === SENT, J(await box(p2)));
  await setReply(p2, card({ status: 'done', event_ts: nowSec() - 1 }));
  await fire(p2, 'records', null);
  await sleep(250);
  b = await box(p2);
  check('D8 a records event whose card has a new event_ts shows "bravo picked it up." in #f5f5f5', b.hint === 'bravo picked it up.' && b.color === WHITE, J(b));
  await sleep(100);
  check('D9 the picked up state stays until typing starts or the card closes', (await box(p2)).hint === 'bravo picked it up.', J(await box(p2)));
  await clickBox(p2);
  check('D10 typing again clears the picked up state', (await box(p2)).hint === HINT, J(await box(p2)));
  await p2.close();

  // picked up: status turns working
  const p3 = await openPage({ tag: 'delivery-status' });
  const ts = nowSec() - 30;
  await openCard(p3, card({ status: 'done', event_ts: ts }));
  await typeAndSend(p3, 'go');
  await setReply(p3, card({ status: 'working', event_ts: ts }));
  await fire(p3, 'records', null);
  await sleep(250);
  b = await box(p3);
  check('D11 the card turning working, with the same event_ts, is a pickup', b.hint === 'bravo picked it up.' && b.color === WHITE, J(b));
  await p3.close();

  // a card already working at the send is not a pickup by itself
  const p4 = await openPage({ tag: 'delivery-already-working' });
  await openCard(p4, card({ status: 'working', event_ts: ts }));
  await typeAndSend(p4, 'go');
  await fire(p4, 'records', null);
  await sleep(250);
  check('D12 a card that was working at the send, unchanged, is not a pickup', (await box(p4)).hint === SENT, J(await box(p4)));
  await p4.close();

  // the pickup check also runs on the card already drawn when the call resolves
  const p5 = await openPage({ tag: 'delivery-late' });
  await openCard(p5, card({ status: 'done', event_ts: ts }));
  await holdPrompt(p5);
  await clickBox(p5);
  await p5.keyboard.type('late');
  await p5.keyboard.press('Enter');
  await sleep(100);
  await setReply(p5, card({ status: 'done', event_ts: nowSec() }));
  await fire(p5, 'records', null);
  await sleep(250);
  check('D13 a pickup seen while the call is in flight leaves the hint at "Sending…"', (await box(p5)).hint === 'Sending\u2026', J(await box(p5)));
  await p5.evaluate(() => window.__promptSettle.ok(null));
  await sleep(150);
  b = await box(p5);
  check('D14 when the call resolves the card already drawn counts: "bravo picked it up."', b.hint === 'bravo picked it up.' && b.color === WHITE, J(b));
  await p5.close();

  // failed
  const p6 = await openPage({ tag: 'delivery-failed' });
  await openCard(p6);
  await p6.evaluate(() => { window.__promptReply = () => Promise.reject('inbox is not a socket'); });
  await typeAndSend(p6, 'keep this');
  b = await box(p6);
  check('D15 a rejected call shows "This session isn\'t reachable." in amber and keeps the draft', b.hint === 'This session isn\'t reachable.' && b.color === AMBER && b.value === 'keep this' && b.send === true, J(b));
  await clickBox(p6);
  b = await box(p6);
  check('D16 typing again clears the failed state and the draft is still there', b.hint === HINT && b.value === 'keep this', J(b));
  await p6.keyboard.press('Enter');
  await sleep(100);
  check('D17 Return sends the kept draft again', J(await invokes(p6, 'prompt')) === J([{ id: 'id-two', text: 'keep this' }, { id: 'id-two', text: 'keep this' }]), J(await invokes(p6, 'prompt')));
  await p6.close();

  // a send in flight: the send button cannot send twice
  const p7 = await openPage({ tag: 'delivery-once' });
  await openCard(p7);
  await holdPrompt(p7);
  await typeAndSend(p7, 'once');
  await p7.locator('#send').click({ timeout: 2000, force: true });
  await sleep(100);
  check('D18 no second prompt call while one is in flight', (await invokes(p7, 'prompt')).length === 1, J(await invokes(p7, 'prompt')));
  await p7.close();

  // closing the card clears the state
  const p8 = await openPage({ tag: 'delivery-close' });
  await openCard(p8);
  await typeAndSend(p8, 'bye');
  await p8.locator('#back').click({ timeout: 2000 });
  await sleep(500);
  await p8.locator('#rows button.srow').nth(1).click({ timeout: 2000 });
  await sleep(700);
  b = await box(p8);
  check('D19 closing the card and opening it again clears the delivery state and the draft', b.hint === '' && b.value === '', J(b));
  await p8.close();
}

const phaseOf = (page) => page.evaluate(() => document.body.dataset.phase);
const FAILED = 'This session isn\'t reachable.';

// CONTRACT.md section 11 "Sending": a send in flight keeps the card open, and a failure while the pointer is away
// holds it open until the pointer comes back.
async function scenarioHold() {
  // away, held, resolved: stays open while sending, then collapses
  const a = await openPage({ tag: 'hold-success' });
  await openCard(a);
  await holdPrompt(a);
  await clickBox(a);
  await a.keyboard.type('away');
  await fire(a, 'hover', false);
  await a.keyboard.press('Enter');
  await sleep(800);
  let b = await box(a);
  check('H1 pointer away, send in flight 800 ms: the panel is still expanded and the hint reads "Sending…"', (await phaseOf(a)) === 'expanded' && b.hint === 'Sending\u2026', J({ phase: await phaseOf(a), b }));
  await a.evaluate(() => window.__promptSettle.ok(null));
  await sleep(150);
  b = await box(a);
  check('H2 resolved with the pointer away: the hint reads "Sent. Waiting for bravo to pick it up."', b.hint === SENT && b.color === MUTED && b.value === '', J(b));
  check('H3 and the phase reaches collapsed within 1.5 s', await waitPhase(a, 'collapsed', 1500), J(await phaseOf(a)));
  await a.close();

  // away, held, rejected: stays open until the pointer returns and leaves
  const f = await openPage({ tag: 'hold-failure' });
  await openCard(f);
  await holdPrompt(f);
  await clickBox(f);
  await f.keyboard.type('keep me');
  await fire(f, 'hover', false);
  await f.keyboard.press('Enter');
  await sleep(800);
  await f.evaluate(() => window.__promptSettle.no('inbox is not a socket'));
  await sleep(1000);
  b = await box(f);
  check('H4 pointer away, rejected: 1 s later the panel is still expanded, the draft is kept and the hint reads "This session isn\'t reachable." in amber', (await phaseOf(f)) === 'expanded' && b.value === 'keep me' && b.hint === FAILED && b.color === AMBER, J({ phase: await phaseOf(f), b }));
  await fire(f, 'hover', true);
  await sleep(600);
  check('H5 the pointer coming back keeps it open with the failed hint', (await phaseOf(f)) === 'expanded' && (await box(f)).hint === FAILED, J({ phase: await phaseOf(f), b: await box(f) }));
  await fire(f, 'hover', false);
  check('H6 a hover out after that collapses it', await waitPhase(f, 'collapsed', 2000), J(await phaseOf(f)));
  await f.close();

  // pointer on the panel, rejected: nothing is held
  const o = await openPage({ tag: 'hold-on-panel' });
  await openCard(o);
  await holdPrompt(o);
  await typeAndSend(o, 'here');
  await o.evaluate(() => window.__promptSettle.no('inbox is not a socket'));
  await sleep(300);
  b = await box(o);
  check('H7 pointer on the panel, rejected: the panel stays expanded with the failed hint and the draft', (await phaseOf(o)) === 'expanded' && b.hint === FAILED && b.value === 'here', J({ phase: await phaseOf(o), b }));
  await fire(o, 'hover', false);
  check('H8 a later hover out collapses it as usual', await waitPhase(o, 'collapsed', 2000), J(await phaseOf(o)));
  await o.close();

  // pointer leaves, returns, leaves again while sending: the pointer is away at the failure
  const r = await openPage({ tag: 'hold-reenter' });
  await openCard(r);
  await holdPrompt(r);
  await clickBox(r);
  await r.keyboard.type('again');
  await fire(r, 'hover', false);
  await r.keyboard.press('Enter');
  await sleep(200);
  await fire(r, 'hover', true);
  await sleep(100);
  await fire(r, 'hover', false);
  await sleep(500);
  await r.evaluate(() => window.__promptSettle.no('inbox is not a socket'));
  await sleep(1000);
  b = await box(r);
  check('H9 hover true then false while sending, then rejected: it still holds, expanded with the draft and the failed hint', (await phaseOf(r)) === 'expanded' && b.value === 'again' && b.hint === FAILED && b.color === AMBER, J({ phase: await phaseOf(r), b }));
  await r.close();
  // typing again during the send, pointer away, rejected, Esc: the failed-and-unseen hold survives the typing
  const t = await openPage({ tag: 'hold-typing' });
  await openCard(t);
  await holdPrompt(t);
  await typeAndSend(t, 'first');
  await clickBox(t);
  await t.keyboard.type('more');
  check('H10 clicking the box during the send takes focus: focus({on: true}) was called twice', J(await invokes(t, 'focus')) === J([{ on: true }, { on: false }, { on: true }]), J(await invokes(t, 'focus')));
  await fire(t, 'hover', false);
  await t.evaluate(() => window.__promptSettle.no('inbox is not a socket'));
  await sleep(100);
  await t.keyboard.press('Escape');
  await sleep(800);
  b = await box(t);
  check('H11 rejected with the pointer away, then Esc: 800 ms later still expanded, the draft typed during the send is in the box, and the hint reads "This session isn\'t reachable." in amber', (await phaseOf(t)) === 'expanded' && b.value === 'firstmore' && b.hint === FAILED && b.color === AMBER, J({ phase: await phaseOf(t), b }));
  await fire(t, 'hover', true);
  await fire(t, 'hover', false);
  check('H12 hover true then false after that collapses within 1 s', await waitPhase(t, 'collapsed', 1000), J(await phaseOf(t)));
  await t.close();

  // a held failure ends when the session ends or its reach leaves inbox
  for (const [name, redraw] of [['session ended', null], ['reach none', card({ reach: { kind: 'none' } })]]) {
    const e = await openPage({ tag: `hold-end-${name}` });
    await openCard(e);
    await holdPrompt(e);
    await clickBox(e);
    await e.keyboard.type('gone');
    await fire(e, 'hover', false);
    await e.keyboard.press('Enter');
    await sleep(300);
    await e.evaluate(() => window.__promptSettle.no('inbox is not a socket'));
    await sleep(800);
    check(`H13 held failure, then ${name} is not yet drawn: still expanded`, (await phaseOf(e)) === 'expanded', J(await phaseOf(e)));
    await setReply(e, redraw);
    await fire(e, 'records', null);
    check(`H14 a redraw showing ${name} ends the hold: collapsed within 1 s`, await waitPhase(e, 'collapsed', 1000), J(await phaseOf(e)));
    await e.close();
  }
}

async function scenarioReach() {
  for (const [name, reach, hint] of [
    ['none', { kind: 'none' }, NO_INBOX],
  ]) {
    const page = await openPage({ tag: `reach-${name}` });
    await openCard(page, card({ reach }));
    const b = await box(page);
    check(`R1 reach ${name}: the textarea is disabled with the placeholder "This session can't take messages" in #6f6f6f`, b.disabled === true && b.placeholder === 'This session can\'t take messages' && b.phColor === 'rgb(111, 111, 111)', J(b));
    check(`R2 reach ${name}: the send button is disabled and the hint reads the no inbox text`, b.send === false && b.hint === hint && b.boxShown && b.hintShown, J(b));
    await page.locator('#pbox').click({ timeout: 2000, force: true });
    await page.mouse.click(...(await page.evaluate(() => { const e = document.querySelector('#pbox').getBoundingClientRect(); return [e.left + 4, e.top + e.height / 2]; })));
    await sleep(200);
    const after = await box(page);
    check(`R3 reach ${name}: a press on the box starts no typing and calls no focus`, !after.typing && (await invokes(page, 'focus')).length === 0 && after.hint === hint, J({ after, f: await invokes(page, 'focus') }));
    await page.keyboard.press('Enter');
    await sleep(100);
    check(`R4 reach ${name}: Return sends nothing`, (await invokes(page, 'prompt')).length === 0, J(await invokes(page, 'prompt')));
    await page.close();
  }

  // host_only (CONTRACT.md "Opening the session's app"): the box is hidden and the Open button takes its place;
  // tests/page/open-host-check.mjs checks the button in full.
  const ho = await openPage({ tag: 'reach-host_only' });
  await openCard(ho, card({ reach: { kind: 'host_only', host: 'com.apple.Terminal' }, app: 'Terminal' }));
  const hb = await box(ho);
  const btn = await ho.evaluate(() => { const e = document.querySelector('#openApp'); return e && !e.hidden && getComputedStyle(e).display !== 'none' && e.getBoundingClientRect().height > 0 ? e.textContent : null; });
  check('R1h reach host_only: #pbox is hidden and #openApp shows "Open Terminal"', !hb.boxShown && btn === 'Open Terminal', J({ hb, btn }));
  check('R2h reach host_only: the hint reads "It has no inbox, so notch-hud can only bring Terminal forward."', hb.hintShown && hb.hint === 'It has no inbox, so notch-hud can only bring Terminal forward.', J(hb));
  await sleep(100);
  await ho.keyboard.press('Enter');
  await sleep(100);
  check('R4h reach host_only: Return sends nothing and calls no focus', (await invokes(ho, 'prompt')).length === 0 && (await invokes(ho, 'focus')).length === 0, J({ p: await invokes(ho, 'prompt'), f: await invokes(ho, 'focus') }));
  await ho.close();

  const page = await openPage({ tag: 'reach-change' });
  await openCard(page);
  await clickBox(page);
  await page.keyboard.type('half');
  await setReply(page, card({ reach: { kind: 'none' } }));
  await fire(page, 'records', null);
  await sleep(300);
  const b = await box(page);
  check('R5 a reach that changes away from inbox while typing ends typing: focus({on: false}), disabled box, no inbox hint', !b.typing && b.disabled === true && b.hint === NO_INBOX && J(await invokes(page, 'focus')) === J([{ on: true }, { on: false }]), J({ b, f: await invokes(page, 'focus') }));
  await setReply(page, card());
  await fire(page, 'records', null);
  await sleep(300);
  const c = await box(page);
  check('R6 a reach that returns to inbox enables the box again', c.disabled === false && c.placeholder === 'Message bravo' && c.hint === '', J(c));
  await page.close();
}

for (const s of [scenarioLook, scenarioType, scenarioPadding, scenarioHoverOut, scenarioIme, scenarioEscStays, scenarioKeyFalse, scenarioLeaveCard, scenarioNoCard, scenarioDelivery, scenarioHold, scenarioReach]) {
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

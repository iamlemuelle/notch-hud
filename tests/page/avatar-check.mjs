// Loads dist/index.html in headless Chromium with a fake Tauri bridge and checks CONTRACT.md section 10 "Page": the
// #avatar box and #total places, the amber total, which animation plays, the paint budget (a MutationObserver on
// #avatar counts every paint), the mood event, and the fallback to the dot. Real time, no retries. Every number comes
// from the contract text. What a given expression looks like is read from the page's own
// `AvatarCore.renderAvatarDefinition(definition, key)`: the svg shows `key` when its path `d` attributes include the
// scene's head and eye paths.
//
// Fixtures: tests/avatar/fixtures/*.json (original definitions; expressions have distinct geometry).
// Env: PLAYWRIGHT_CORE, CHROMIUM, NOTCH_PAGE as in tests/page_test.sh. Exit 0 when every check passes, 1 when one
// fails, 2 when the browser cannot start.
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const repo = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PW = process.env.PLAYWRIGHT_CORE || '/Users/lem/.npm/_npx/a1722018f28f9df2/node_modules/playwright-core';
const CHROMIUM = process.env.CHROMIUM || '/Users/lem/Library/Caches/ms-playwright/chromium_headless_shell-1208/chrome-headless-shell-mac-arm64/chrome-headless-shell';
const PAGE_FILE = resolve(process.env.NOTCH_PAGE || join(repo, 'dist', 'index.html'));
const fixture = (name) => JSON.parse(readFileSync(join(repo, 'tests', 'avatar', 'fixtures', name + '.json'), 'utf8'));

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

// ---------------------------------------------------------------------------------------------------------------
// Contract numbers. Stage 452 wide, pill 305 x 32 (a 185 pt notch), open 420 x 254 for one row.
const LAYOUT = {
  stage: { width: 452, height: 982 },
  pill: { width: 305, height: 32 },
  peek: { width: 309, height: 36 },
  banner: { width: 380, height: 78 },
  open: { width: 420, height: 254 },
  ear: 60,
};
const WINDOW = { pill: { w: 305, h: 32 }, banner: { w: 412, h: 94 }, open: { w: 452, h: 270 } };
const PILL_L = 73.5;
const CONTENT_L = 16;
const AMBER = 'rgb(245, 166, 35)';
const TEXT = 'rgb(245, 245, 245)';
const GREEN = 'rgb(52, 208, 88)';
const GRAY = 'rgb(139, 139, 139)';
// Section 10's mood table: animations and still.
const POSE = {
  alert: { mood: 'alert', animations: ['surprised', 'curious'], still: false },
  error: { mood: 'error', animations: ['sad', 'angry'], still: false },
  celebrate: { mood: 'celebrate', animations: ['celebrate', 'happy', 'excited'], still: false },
  working: { mood: 'working', animations: ['working', 'thinking'], still: false },
  listening: { mood: 'listening', animations: ['listening'], still: false },
  idle: { mood: 'idle', animations: ['idle'], still: false },
  drowsy: { mood: 'drowsy', animations: ['drowsy', 'bored'], still: false },
  sleeping: { mood: 'sleeping', animations: ['sleeping'], still: true },
};
// What each animation of tests/avatar/fixtures/ball.json shows (its steps' expressions).
const BALL = fixture('ball');
const KEYS = { idle: ['calm', 'wide'], working: ['happy', 'shy'], sleeping: ['sad'], curious: ['grin', 'peek'], celebrate: ['wow', 'peek'] };

const results = [];
const check = (name, ok, detail) => results.push({ name, ok: !!ok, detail: ok ? undefined : detail });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const near = (v, target, tol) => typeof v === 'number' && Math.abs(v - target) <= tol;
const J = (v) => JSON.stringify(v);
const pageErrors = [];

// Installed before any page script: the fake bridge and the recorder of mutations inside #avatar.
function installBridge({ layout, windows, avatar, mood, holdMood, noCore }) {
  const handlers = {};
  const now = () => performance.now();
  const rec = (window.__rec = { invokes: [], surfaces: [] });
  window.__fire = (name, payload) => { (handlers[name] || []).forEach((h) => h({ payload })); };
  let releaseMood;
  window.__releaseMood = (pose) => releaseMood && releaseMood(pose);
  window.__TAURI__ = {
    event: { listen: (name, h) => { (handlers[name] = handlers[name] || []).push(h); return Promise.resolve(() => {}); } },
    core: {
      invoke: (cmd, args) => {
        rec.invokes.push({ cmd, t: now() });
        if (cmd === 'layout') return Promise.resolve(layout);
        if (cmd === 'avatar') return avatar === 'reject' ? Promise.reject(new Error('no avatar')) : Promise.resolve(avatar);
        if (cmd === 'mood') return holdMood ? new Promise((r) => { releaseMood = (p) => r(p || mood); }) : Promise.resolve(mood);
        if (cmd === 'surface') {
          const w = args && windows[args.window];
          return new Promise((resolve) => setTimeout(async () => { if (w) await window.__setWindow(w.w, w.h); resolve(null); }, 0));
        }
        return Promise.resolve(null);
      },
    },
  };
  if (noCore) Object.defineProperty(window, 'AvatarCore', { configurable: false, get() { return undefined; }, set() {} });

  // One entry per observer callback that touched #avatar: a paint is one task, so one callback.
  const mut = (window.__mut = { batches: [], records: 0 });
  new MutationObserver((ms) => {
    const hits = ms.filter((m) => { const n = m.target.nodeType === 1 ? m.target : m.target.parentElement; return n && n.closest && n.closest('#avatar'); });
    if (hits.length) { mut.batches.push(now()); mut.records += hits.length; }
  }).observe(document, { subtree: true, childList: true, attributes: true, characterData: true });

  // Box relative to #stage, effective visibility, computed color.
  window.__box = (sel) => {
    const e = document.querySelector(sel);
    const sr = document.querySelector('#stage').getBoundingClientRect();
    if (!e) return null;
    const r = e.getBoundingClientRect();
    let eff = 1;
    for (let n = e; n && n.nodeType === 1; n = n.parentElement) {
      const c = getComputedStyle(n);
      if (c.display === 'none' || c.visibility === 'hidden') { eff = 0; break; }
      eff *= parseFloat(c.opacity);
    }
    const cs = getComputedStyle(e);
    return { x: r.left - sr.left, y: r.top - sr.top, w: r.width, h: r.height, eff: r.width > 0 && r.height > 0 ? eff : 0, color: cs.color, bg: cs.backgroundColor, text: e.textContent.trim() };
  };
  // The keys, of those given, that the svg shows right now.
  window.__shown = (def, keys) => {
    const ds = new Set([...document.querySelectorAll('#avatar path')].map((p) => p.getAttribute('d')));
    return keys.filter((k) => {
      const g = window.AvatarCore.renderAvatarDefinition(def, k).geometry;
      return ds.has(g.headPath) && ds.has(g.leftPath) && ds.has(g.rightPath);
    });
  };
}

async function openPage(opts = {}) {
  const page = await browser.newPage({ viewport: { width: WINDOW.pill.w, height: WINDOW.pill.h } });
  const tag = opts.tag || 'page';
  page.on('pageerror', (e) => pageErrors.push(`${tag}: pageerror: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error' && !opts.expectErrors) pageErrors.push(`${tag}: console: ${m.text()}`); });
  if (opts.reduced) await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.exposeFunction('__setWindow', (w, h) => page.setViewportSize({ width: w, height: h }).catch(() => {}));
  await page.addInitScript(installBridge, {
    layout: LAYOUT, windows: WINDOW,
    avatar: opts.avatar === 'reject' ? 'reject' : { source: 'default', definition: opts.def || BALL },
    mood: opts.pose || POSE.idle, holdMood: !!opts.holdMood, noCore: !!opts.noCore,
  });
  await page.goto('file://' + PAGE_FILE);
  await page.waitForFunction(() => document.body && document.body.dataset.phase, null, { timeout: 1500 }).catch(() => {});
  return page;
}

const fire = (page, name, payload) => page.evaluate(([n, p]) => window.__fire(n, p), [name, payload]);
const box = (page, sel) => page.evaluate((s) => window.__box(s), sel);
const shown = (page, def, keys) => page.evaluate(([d, k]) => window.__shown(d, k), [def, keys]).catch(() => []);
const waitPhase = (page, phase, ms) => page.waitForFunction((p) => document.body.dataset.phase === p, phase, { timeout: ms, polling: 20 }).then(() => true, () => false);
const waitTransitionsDone = (page, ms) => page.waitForFunction(() => document.getAnimations().every((a) => !(a instanceof CSSTransition) || a.playState !== 'running'), null, { timeout: ms, polling: 20 }).then(() => true, () => false);
const painted = (page, ms = 1500) => page.waitForFunction(() => document.querySelectorAll('#avatar path').length > 0, null, { timeout: ms, polling: 20 }).then(() => true, () => false);
const row = (label, status, detail = null) => ({ label, status, detail });
const summary = (rows) => ({
  rows, total: rows.length,
  attention: rows.filter((r) => ['needs_permission', 'needs_input', 'error'].includes(r.status)).length,
  working: rows.filter((r) => r.status === 'working').length,
});
const nowOf = (page) => page.evaluate(() => performance.now());
// Observer callbacks that touched #avatar during the next `ms`.
async function observe(page, ms) {
  const t0 = await nowOf(page);
  await sleep(ms);
  return page.evaluate((t) => window.__mut.batches.filter((b) => b >= t), t0);
}
// The keys seen, sampling every 40 ms for `ms`.
async function seen(page, def, keys, ms) {
  const out = new Set();
  const end = Date.now() + ms;
  while (Date.now() < end) { (await shown(page, def, keys)).forEach((k) => out.add(k)); await sleep(40); }
  return [...out].sort();
}
// Waits until the svg shows one of the keys; returns it or null.
async function waitShown(page, def, keys, ms) {
  const end = Date.now() + ms;
  while (Date.now() < end) { const s = await shown(page, def, keys); if (s.length) return s[0]; await sleep(30); }
  return null;
}
// Groups batch times into clusters separated by more than `gap` ms.
function clusters(times, gap) {
  const out = [];
  for (const t of times) {
    const c = out[out.length - 1];
    if (c && t - c.end <= gap) { c.end = t; c.n += 1; } else out.push({ start: t, end: t, n: 1 });
  }
  return out;
}
const minGap = (times) => times.slice(1).reduce((m, t, i) => Math.min(m, t - times[i]), Infinity);
const all = Object.values(KEYS).flat().filter((k, i, a) => a.indexOf(k) === i);

// ---------------------------------------------------------------------------------------------------------------
async function scenarioPlaces() {
  const page = await openPage({ tag: 'places' });
  check('P0 the page asks for avatar and mood at load', await page.evaluate(() => ['avatar', 'mood'].every((c) => window.__rec.invokes.some((i) => i.cmd === c))), 'no avatar and mood invoke');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await painted(page);
  await sleep(300);
  const check_collapsed = async (label) => {
    const a = await box(page, '#avatar');
    const t = await box(page, '#total');
    check(`P1 ${label}: #avatar is 24 x 24 at stage (${PILL_L + 12}, 4)`, !!a && near(a.x, PILL_L + 12, 1) && near(a.y, 4, 1) && near(a.w, 24, 1) && near(a.h, 24, 1), J(a));
    check(`P2 ${label}: #total's left edge is at pillL + 41 = ${PILL_L + 41} and its center is at y 16`, !!t && near(t.x, PILL_L + 41, 1) && near(t.y + t.h / 2, 16, 1), J(t));
  };
  await check_collapsed('collapsed');
  const dot = await box(page, '#dot');
  check('P3 the dot is not shown while the avatar is', !dot || dot.eff === 0, J(dot));
  const av = await box(page, '#avatar');
  check('P4 #avatar is an svg with viewBox "-150 -150 300 300"', await page.evaluate(() => { const e = document.querySelector('#avatar'); return e && e.tagName.toLowerCase() === 'svg' && e.getAttribute('viewBox') === '-150 -150 300 300'; }), J(av));

  // A banner opens (the status of a row changed to needs_permission): every phase but expanded uses the collapsed place.
  await fire(page, 'sessions', summary([row('paxiv', 'needs_permission', 'Bash')]));
  const gotBanner = await waitPhase(page, 'banner', 1500);
  await waitTransitionsDone(page, 1500);
  await sleep(150);
  const ab = await box(page, '#avatar');
  const tb = await box(page, '#total');
  check('P5 banner phase: #avatar and #total keep the collapsed place', gotBanner && !!ab && !!tb && near(ab.x, PILL_L + 12, 1) && near(ab.y, 4, 1) && near(ab.w, 24, 1) && near(tb.x, PILL_L + 41, 1), J({ gotBanner, ab, tb }));
  await waitPhase(page, 'collapsed', 4000);
  await waitTransitionsDone(page, 1500);

  await fire(page, 'hover', true);
  const exp = await waitPhase(page, 'expanded', 2000);
  await waitTransitionsDone(page, 1500);
  await sleep(200);
  const ae = await box(page, '#avatar');
  const te = await box(page, '#total');
  check(`P6 expanded: #avatar is 36 x 36 at stage (${CONTENT_L + 16}, 4)`, exp && !!ae && near(ae.x, CONTENT_L + 16, 1) && near(ae.y, 4, 1) && near(ae.w, 36, 1) && near(ae.h, 36, 1), J({ exp, ae }));
  check(`P7 expanded: #total's left edge is at contentL + 58 = ${CONTENT_L + 58} and its center is at y 22`, !!te && near(te.x, CONTENT_L + 58, 1) && near(te.y + te.h / 2, 22, 1), J(te));
  await fire(page, 'hover', false);
  await waitPhase(page, 'collapsed', 3000);
  await waitTransitionsDone(page, 1500);
  await sleep(200);
  await check_collapsed('collapsed again');
  await page.close();
}

async function scenarioTotalColor() {
  const page = await openPage({ tag: 'total-color' });
  await painted(page);
  const color = async () => (await box(page, '#total')).color;
  const cases = [
    ['needs_input', 1, AMBER], ['needs_permission', 1, AMBER], ['error', 1, AMBER],
    ['working', 0, TEXT], ['done', 0, TEXT], ['waiting', 0, TEXT],
  ];
  for (const [status, attention, want] of cases) {
    await fire(page, 'sessions', summary([row('s', status)]));
    await sleep(120);
    const c = await color();
    check(`T1 a ${status} row (attention ${attention}): #total is ${attention ? 'amber rgb(245, 166, 35)' : '#f5f5f5'}`, c === want, c);
  }
  await fire(page, 'sessions', summary([]));
  await sleep(120);
  check('T2 no sessions: #total is #f5f5f5 and reads 0', (await color()) === TEXT && (await box(page, '#total')).text === '0', J(await box(page, '#total')));
  await fire(page, 'sessions', summary([row('a', 'needs_input'), row('b', 'working'), row('c', 'done')]));
  await sleep(120);
  const t = await box(page, '#total');
  check('T3 a mix with one needing you: amber and reads 3', t.color === AMBER && t.text === '3', J(t));
  await page.close();
}

async function scenarioWhichAnimation() {
  let page = await openPage({ tag: 'which', pose: POSE.alert });
  await painted(page);
  let got = await seen(page, BALL, all, 2300);
  check('W1 alert (surprised, curious) on a definition with only curious: it plays curious and loops (shows grin and peek)', J(got) === J(['grin', 'peek']), J(got));
  await page.close();

  const both = structuredClone(BALL);
  both.animations.surprised = structuredClone(BALL.animations.working);
  both.animationOrder.push('surprised');
  page = await openPage({ tag: 'which2', pose: POSE.alert, def: both });
  await painted(page);
  got = await seen(page, both, all, 2300);
  check('W2 a definition with surprised and curious: the first name that exists plays (surprised: happy, shy; never grin or peek)', J(got) === J(['happy', 'shy']), J(got));
  await page.close();

  page = await openPage({ tag: 'which3', pose: POSE.idle });
  await painted(page);
  got = await seen(page, BALL, all, 2300);
  check('W3 idle plays the idle animation (calm and wide)', J(got) === J(['calm', 'wide']), J(got));
  await page.close();
}

async function scenarioStillAndReduced() {
  // still pose
  let page = await openPage({ tag: 'still', pose: POSE.sleeping });
  await painted(page);
  await sleep(400);
  let s = await shown(page, BALL, all);
  check('S1 a still pose (sleeping) paints its first step\'s expression (sad)', J(s) === J(['sad']), J(s));
  let b = await observe(page, 2500);
  check('S2 ... and then never mutates the svg again (2.5 s)', b.length === 0, `${b.length} mutation callbacks`);
  await page.close();

  // no matching animation: neutral
  page = await openPage({ tag: 'nomatch', def: fixture('nomatch'), pose: POSE.idle });
  await painted(page);
  await sleep(400);
  const nm = fixture('nomatch');
  s = await shown(page, nm, ['neutral', 'wow', 'peek']);
  check('S3 no animation of the pose in the definition: paints neutral', J(s) === J(['neutral']), J(s));
  b = await observe(page, 2500);
  check('S4 ... and never mutates the svg again (2.5 s)', b.length === 0, `${b.length} mutation callbacks`);
  await page.close();

  // reduced motion
  page = await openPage({ tag: 'reduced', reduced: true, pose: POSE.idle });
  await painted(page);
  await sleep(400);
  s = await shown(page, BALL, all);
  check('S5 reduced motion: idle paints the first step\'s expression (calm)', J(s) === J(['calm']), J(s));
  b = await observe(page, 2500);
  check('S6 ... and never mutates the svg again (2.5 s)', b.length === 0, `${b.length} mutation callbacks`);
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  const k = await seen(page, BALL, all, 2300);
  check('S7 reduced motion switched off while running: the animation starts (calm and wide)', J(k) === J(['calm', 'wide']), J(k));
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await sleep(700);
  b = await observe(page, 2500);
  check('S8 reduced motion switched on again: the svg stops mutating', b.length === 0, `${b.length} mutation callbacks`);
  await page.close();
}

async function scenarioBudget() {
  // holds with no blink and no ambient motion: paints only during a transition
  let page = await openPage({ tag: 'budget' });
  await painted(page);
  await sleep(1200);
  let b = await observe(page, 4000);
  const cs = clusters(b, 150);
  const tooLong = cs.filter((c) => c.end - c.start > 250 || c.n > 6);
  const spacing = cs.slice(1).map((c, i) => c.start - cs[i].start);
  check('B1 a loop of 400 ms holds with 100 ms transitions: frames come in bursts (6 or more over 4 s), each at most 250 ms long',
    cs.length >= 6 && tooLong.length === 0, J({ bursts: cs.length, tooLong, total: b.length }));
  check('B2 ... and nothing mutates during a hold: bursts start at least 400 ms apart', spacing.length > 0 && spacing.every((x) => x >= 400), J(spacing));
  check('B3 ... never more than one frame per 50 ms (gaps of at least 45 ms)', b.length > 0 && minGap(b) >= 45, `min gap ${minGap(b)} over ${b.length}`);
  await page.close();

  // ambient motion: frames continue through a hold, still at most 20/s
  const drift = fixture('drift');
  page = await openPage({ tag: 'drift', def: drift });
  await painted(page);
  await sleep(500);
  b = await observe(page, 1500);
  check('B4 ambient motion (microSaccades, slowDrift) in a 2 s hold: frames keep coming (at least 8 in 1.5 s)', b.length >= 8, `${b.length} in 1.5 s`);
  check('B5 ... at most 20 per second (30 in 1.5 s, gaps of at least 45 ms)', b.length <= 31 && minGap(b) >= 45, `${b.length} in 1.5 s, min gap ${minGap(b)}`);
  await page.close();

  // blink
  page = await openPage({ tag: 'blink', def: fixture('blink') });
  await painted(page);
  await sleep(600);
  b = await observe(page, 3500);
  const bc = clusters(b, 120);
  const gaps = bc.slice(1).map((c, i) => c.start - bc[i].end);
  check('B6 blinking in a 5 s hold: frames come in bursts (at least 3 blinks in 3.5 s), each at most 300 ms long', bc.length >= 3 && bc.every((c) => c.end - c.start <= 300), J({ bursts: bc.length, long: bc.filter((c) => c.end - c.start > 300) }));
  check('B7 ... with no frames between blinks (bursts at least 250 ms apart) and at most one per 50 ms', gaps.length > 0 && gaps.every((g) => g >= 250) && minGap(b) >= 45, J({ gaps, minGap: minGap(b) }));
  await page.close();

  // a once animation that ended keeps its last frame and no timer
  page = await openPage({ tag: 'once', pose: POSE.celebrate });
  await painted(page);
  await sleep(2200);
  b = await observe(page, 2500);
  const s = await shown(page, BALL, all);
  check('B8 a once animation (celebrate) that ended keeps its last frame (peek) and sets no timer', b.length === 0 && J(s) === J(['peek']), J({ mutations: b.length, shown: s }));
  await page.close();
}

async function scenarioMood() {
  const page = await openPage({ tag: 'mood', pose: POSE.idle });
  await painted(page);
  await sleep(500);
  let k = await seen(page, BALL, all, 1200);
  check('M1 starts as idle (calm and wide)', J(k) === J(['calm', 'wide']), J(k));

  await fire(page, 'mood', POSE.sleeping);
  let first = await waitShown(page, BALL, ['sad'], 800);
  await sleep(300);
  let b = await observe(page, 1500);
  check('M2 a mood event with a still pose paints sad, and then nothing mutates', first === 'sad' && b.length === 0, J({ first, mutations: b.length }));

  await fire(page, 'mood', POSE.working);
  k = await seen(page, BALL, all, 1800);
  check('M3 a mood event with the working pose plays working (happy and shy)', J(k) === J(['happy', 'shy']), J(k));

  await fire(page, 'mood', POSE.alert);
  k = await seen(page, BALL, all, 1800);
  check('M4 a mood event with the alert pose plays curious (grin and peek)', J(k) === J(['grin', 'peek']), J(k));

  await fire(page, 'mood', POSE.idle);
  k = await seen(page, BALL, all, 1800);
  check('M5 a mood event back to idle plays idle (calm and wide)', J(k) === J(['calm', 'wide']), J(k));
  await page.close();

  // The reply to the startup `mood` call is older than an event that arrived first.
  const late = await openPage({ tag: 'mood-late', holdMood: true, pose: POSE.idle });
  await painted(late);
  await fire(late, 'mood', POSE.sleeping);
  await waitShown(late, BALL, ['sad'], 800);
  await late.evaluate(() => window.__releaseMood());
  await sleep(700);
  const s = await shown(late, BALL, all);
  check('M6 a startup reply that arrives after a mood event is ignored (still sad)', J(s) === J(['sad']), J(s));
  await late.close();
}

async function scenarioFallback() {
  const cases = [
    ['no AvatarCore', { noCore: true }],
    ['the avatar reply rejects', { avatar: 'reject', expectErrors: true }],
    ['painting throws', { def: { schema: 'bible-strong/avatar-definition' }, expectErrors: true }],
  ];
  for (const [name, opts] of cases) {
    const page = await openPage({ tag: 'fallback-' + name, ...opts });
    await fire(page, 'sessions', summary([row('s', 'needs_permission', 'Bash')]));
    await sleep(900);
    const a = await box(page, '#avatar');
    const d = await box(page, '#dot');
    check(`F1 ${name}: #avatar is hidden`, !a || a.eff === 0, J(a));
    check(`F2 ${name}: the dot shows in its place, amber with attention`, !!d && d.eff > 0 && d.bg === AMBER && near(d.w, 8, 0.6), J(d));
    await fire(page, 'sessions', summary([row('s', 'working')]));
    await sleep(150);
    check(`F3 ${name}: green when only working`, (await box(page, '#dot')).bg === GREEN, J(await box(page, '#dot')));
    await fire(page, 'sessions', summary([row('s', 'done')]));
    await sleep(150);
    check(`F4 ${name}: gray when quiet`, (await box(page, '#dot')).bg === GRAY, J(await box(page, '#dot')));
    check(`F5 ${name}: #total is still there`, (await box(page, '#total')).text === '1', J(await box(page, '#total')));
    if (name === 'painting throws') {
      const b = await observe(page, 1500);
      check('F6 painting throws: the avatar\'s timers are stopped (no mutations in 1.5 s)', b.length === 0, `${b.length} mutation callbacks`);
    }
    await page.close();
  }
}

const scenarios = [scenarioPlaces, scenarioTotalColor, scenarioWhichAnimation, scenarioStillAndReduced, scenarioBudget, scenarioMood, scenarioFallback];
for (const s of scenarios) {
  try { await s(); } catch (e) { check(`${s.name} ran to the end`, false, e && e.stack ? e.stack.split('\n').slice(0, 3).join(' | ') : String(e)); }
}
check('X1 no page errors or console errors', pageErrors.length === 0, J(pageErrors.slice(0, 5)));

await browser.close();
for (const r of results) console.log(`${r.ok ? 'PASS' : 'FAIL'} ${r.name}${r.ok ? '' : ' :: ' + r.detail}`);
const failed = results.filter((r) => !r.ok).length;
console.log(`${results.length - failed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);

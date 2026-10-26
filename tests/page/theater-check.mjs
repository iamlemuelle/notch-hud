// Loads dist/index.html in headless Chromium with a fake Tauri bridge and checks CONTRACT.md section 8, "Theater
// (amendment, 2026-10-26)", and section 12, "Page": a file dragged over the notch opens the panel and shows the drop
// target (#drop), a hover out while dragging changes nothing, a drag that ends collapses it; a rejected drop shows
// its error for 2200 ms; a `video` opens the stage (window theater first, the clip to the theater inset, the file's
// name in #tv, the control row, pinned against hover out); a click on #vClose or Esc closes it (phase expanded at
// once, the window stays the stage's until the clip is back, and only then may the panel collapse); `drag` and
// `video-rejected` in the stage change nothing; reduced motion has no clip transition. Every expected value is a
// literal written from the contract text. Real time, no retries.
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

// Contract numbers (section 8, "Theater"): the stage is 640 wide, the theater 608 x 420 on a 32 pt notch.
const LAYOUT = {
  stage: { width: 640, height: 982 },
  pill: { width: 305, height: 32 },
  peek: { width: 309, height: 36 },
  banner: { width: 380, height: 78 },
  open: { width: 420, height: 342 },
  detail: { width: 420, height: 380 },
  theater: { width: 608, height: 420 },
  ear: 60,
};
const WINDOW = { pill: { w: 305, h: 32 }, banner: { w: 412, h: 94 }, open: { w: 452, h: 396 }, theater: { w: 640, h: 436 } };
const AMBER = 'rgb(245, 166, 35)';
const VIDEO = { path: '/Users/lem/Movies/clip.mp4', name: 'clip.mp4' };
const SUMMARY = {
  rows: [{ id: 'id-one', label: 'alpha', status: 'working', detail: null }, { id: 'id-two', label: 'bravo', status: 'working', detail: null }],
  total: 2, attention: 0, working: 2,
};

const results = [];
const check = (name, ok, detail) => results.push({ name, ok: !!ok, detail: ok ? undefined : detail });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const J = (v) => JSON.stringify(v);
const pageErrors = [];

// Installed before any page script: the fake bridge. Besides answering the commands it records, with the page's own
// clock, every phase change, every change of #drop's class, every surface call (with the inline clip-path at that
// moment), every clip-path transition start and end on #panel, every click and every event fired.
function installBridge({ layout, windows }) {
  const handlers = {};
  const now = () => performance.now();
  const rec = (window.__rec = { invokes: [], surfaces: [], phases: [], drop: [], clipRuns: [], clipEnds: [], clicks: [], fired: [] });
  window.__fire = (name, payload) => {
    const t = now();
    rec.fired.push({ name, t });
    (handlers[name] || []).forEach((h) => h({ payload }));
    return t;
  };
  new MutationObserver((records) => {
    for (const m of records) {
      const el = m.target;
      if (el === document.body && m.attributeName === 'data-phase') rec.phases.push({ phase: el.dataset.phase, t: now() });
      else if (el.id === 'drop') rec.drop.push({ on: el.classList.contains('on'), t: now() });
    }
  }).observe(document, { subtree: true, attributes: true, attributeFilter: ['data-phase', 'class'] });
  const onPanel = (list) => (e) => { if (e.target && e.target.id === 'panel' && e.propertyName === 'clip-path') list.push(now()); };
  document.addEventListener('transitionrun', onPanel(rec.clipRuns), true);
  document.addEventListener('transitionend', onPanel(rec.clipEnds), true);
  document.addEventListener('click', () => rec.clicks.push(now()), true);
  window.__TAURI__ = {
    event: { listen: (name, h) => { (handlers[name] = handlers[name] || []).push(h); return Promise.resolve(() => {}); } },
    core: {
      invoke: (cmd, args) => {
        rec.invokes.push({ cmd, args: args ? JSON.parse(JSON.stringify(args)) : null });
        if (cmd === 'layout') return Promise.resolve(layout);
        if (cmd === 'surface') {
          const w = args && windows[args.window];
          const panel = document.getElementById('panel');
          rec.surfaces.push({ window: args.window, hit: args.hit, t: now(), clip: panel ? panel.style.clipPath : '' });
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
  if (opts.reduced) await page.emulateMedia({ reducedMotion: 'reduce' });
  if (process.env.PAGE_CPU_THROTTLE) await (await page.context().newCDPSession(page)).send('Emulation.setCPUThrottlingRate', { rate: Number(process.env.PAGE_CPU_THROTTLE) });
  await page.exposeFunction('__setWindow', (w, h) => page.setViewportSize({ width: w, height: h }).catch(() => {}));
  await page.addInitScript(installBridge, { layout: LAYOUT, windows: WINDOW });
  await page.goto('file://' + PAGE_FILE);
  await page.waitForFunction(() => document.body && document.body.dataset.phase, null, { timeout: 1500 }).catch(() => {});
  await fire(page, 'sessions', SUMMARY);
  return page;
}

const fire = (page, name, payload) => page.evaluate(([n, p]) => window.__fire(n, p), [name, payload]);
const waitPhase = (page, phase, ms) => page.waitForFunction((p) => document.body.dataset.phase === p, phase, { timeout: ms, polling: 20 }).then(() => true, () => false);
const waitTransitionsDone = (page, ms) => page.waitForFunction(() => document.getAnimations().every((a) => !(a instanceof CSSTransition) || a.playState !== 'running'), null, { timeout: ms, polling: 20 }).then(() => true, () => false);
// The clip's spring has ended: one more transitionend on #panel than `ends`.
const waitClipEnd = (page, ends, ms) => page.waitForFunction((n) => window.__rec.clipEnds.length > n, ends, { timeout: ms, polling: 20 }).then(() => true, () => false);
const recording = (page) => page.evaluate(() => JSON.parse(JSON.stringify(window.__rec)));
const pageNow = (page) => page.evaluate(() => performance.now());
const phase = (page) => page.evaluate(() => document.body.dataset.phase);
const viewport = (page) => { const v = page.viewportSize(); return `${v.width}x${v.height}`; };

// Hover in and wait for the open panel with its spring done: the state a video is dropped on.
async function openPanel(page) {
  await fire(page, 'hover', true);
  const ok = await waitPhase(page, 'expanded', 2500);
  await waitTransitionsDone(page, 1500);
  await sleep(150);
  return ok;
}
// A video drop and everything it starts: the window grows, the clip springs, the layers cross.
async function dropVideo(page, reduced = false) {
  const ends = (await recording(page)).clipEnds.length;
  await fire(page, 'video', VIDEO);
  const ok = await waitPhase(page, 'theater', 1000);
  if (reduced) await sleep(400);
  else { await waitClipEnd(page, ends, 2000); await waitTransitionsDone(page, 2000); await sleep(250); }
  return ok;
}

// "inset(0px 16px 562px 16px round 0px 0px 22px 22px)" to its four edges and its radii.
function parseClip(s) {
  const m = /^inset\((.+?)(?:\s+round\s+(.+))?\)$/.exec(String(s ?? '').trim());
  if (!m) return null;
  const four = (list) => {
    const v = list.trim().split(/\s+/).map(parseFloat);
    if (v.some(Number.isNaN)) return null;
    return v.length === 1 ? [v[0], v[0], v[0], v[0]] : v.length === 2 ? [v[0], v[1], v[0], v[1]] : v.length === 3 ? [v[0], v[1], v[2], v[1]] : v.length === 4 ? v : null;
  };
  const edges = four(m[1]), radii = m[2] ? four(m[2]) : [0, 0, 0, 0];
  return edges && radii ? { top: edges[0], right: edges[1], bottom: edges[2], left: edges[3], radii } : null;
}
// The stage's clip for a 640 x 982 stage: sides (640 - w) / 2, bottom 982 - h, radius 22 at the bottom corners.
const clipIs = (s, w, h) => {
  const c = parseClip(s);
  return !!c && c.top === 0 && Math.abs(c.left - (640 - w) / 2) < 0.1 && Math.abs(c.right - (640 - w) / 2) < 0.1
    && Math.abs(c.bottom - (982 - h)) < 0.1 && c.radii[0] === 0 && c.radii[1] === 0 && c.radii[2] === 22 && c.radii[3] === 22;
};
const THEATER_CLIP = [608, 420];
const near = (a, b, e = 0.6) => Math.abs(a - b) <= e;

const look = (page) => page.evaluate(() => {
  const q = (s) => document.querySelector(s), cs = (s) => getComputedStyle(q(s));
  const T = q('#theater').getBoundingClientRect();
  const rel = (s) => { const b = q(s).getBoundingClientRect(); return { x: b.left - T.left, y: b.top - T.top, w: b.width, h: b.height }; };
  return {
    phase: document.body.dataset.phase, name: q('#tvName').textContent, theaterOpacity: cs('#theater').opacity, contentOpacity: cs('#content').opacity,
    clip: cs('#panel').clipPath, tv: rel('#tv'), tctl: rel('#tctl'), tvRadius: cs('#tv').borderRadius, tvBg: cs('#tv').backgroundColor,
    playDisabled: q('#vPlay').disabled, closeLabel: q('#vClose').getAttribute('aria-label'), theaterLeft: T.left,
    dropOn: q('#drop').classList.contains('on'), dropTitle: q('#drop b').textContent, dropSub: q('#dropSub').textContent, dropSubColor: cs('#dropSub').color,
  };
});

// Scenarios ------------------------------------------------------------------------------------------------------

async function scenarioDrag() {
  const page = await openPage({ tag: 'drag' });
  const t0 = await fire(page, 'drag', true);
  const first = await recording(page);
  check('D1 drag true while collapsed: the phase goes to intent first', first.phases.length > 0 && first.phases[0].phase === 'intent' && first.phases[0].t >= t0, J(first.phases));
  check('D2 and on to expanded', await waitPhase(page, 'expanded', 2500), `phase ${await phase(page)}`);
  await waitTransitionsDone(page, 1500);
  const l = await look(page);
  check('D3 #drop has class on, reading "Drop to play in the notch" over "MP4, M4V or MOV"', l.dropOn && l.dropTitle === 'Drop to play in the notch' && l.dropSub === 'MP4, M4V or MOV', J(l));

  const before = (await recording(page)).phases.length;
  await fire(page, 'hover', false);
  await sleep(600);
  const held = await recording(page);
  check('D4 hover false while dragging leaves it expanded for 600 ms, the drop target still shown', (await phase(page)) === 'expanded' && held.phases.length === before && (await look(page)).dropOn, J({ phase: await phase(page), phases: held.phases.slice(before) }));

  await fire(page, 'drag', false);
  const collapsed = await waitPhase(page, 'collapsed', 3000);
  await sleep(100);
  const r = await recording(page);
  const lastSurface = r.surfaces[r.surfaces.length - 1];
  check('D5 drag false with hover false collapses back to collapsed and the window to the pill', collapsed && lastSurface.window === 'pill' && viewport(page) === '305x32' && !(await look(page)).dropOn, J({ collapsed, lastSurface, viewport: viewport(page) }));
  await page.close();
}

async function scenarioRejected() {
  const page = await openPage({ tag: 'rejected' });
  await fire(page, 'drag', true);
  await waitPhase(page, 'expanded', 2500);
  await waitTransitionsDone(page, 1500);
  const tr = await fire(page, 'video-rejected', 'not an mp4, m4v or mov file');
  const l = await look(page);
  check('J1 video-rejected after a drag: #dropSub reads the error in amber and #drop is shown', l.dropOn && l.dropSub === 'That isn\'t a video. Drop an MP4, M4V or MOV.' && l.dropSubColor === AMBER, J(l));
  const phasesAt = (await recording(page)).phases.length;
  const wait = tr + 2000 - (await pageNow(page));
  if (wait > 0) await sleep(wait);
  const mid = await look(page);
  check('J2 the panel stays expanded and the error shown through 2000 ms', mid.phase === 'expanded' && mid.dropOn && mid.dropSub === l.dropSub && (await recording(page)).phases.length === phasesAt, J(mid));
  const collapsed = await waitPhase(page, 'collapsed', 3500);
  const r = await recording(page);
  const off = r.drop.find((d) => d.t > tr && !d.on), collapsing = r.phases.find((p) => p.t > tr && p.phase === 'collapsing');
  check('J3 the drop target hides about 2200 ms after the rejection', !!off && off.t - tr >= 2195 && off.t - tr <= 2700, J({ off: off && off.t - tr }));
  check('J4 the panel then collapses, its first step 250 ms after the target hid, and ends collapsed', collapsed && !!collapsing && !!off && collapsing.t - off.t >= 245 && collapsing.t - tr >= 2445, J({ collapsing: collapsing && collapsing.t - tr, off: off && off.t - tr, collapsed }));
  await page.close();
}

async function scenarioStage() {
  const page = await openPage({ tag: 'stage' });
  check('V0 opens the panel', await openPanel(page), `phase ${await phase(page)}`);
  const before = (await recording(page)).surfaces.length;
  const endsBefore = (await recording(page)).clipEnds.length;
  const tv = await fire(page, 'video', VIDEO);
  check('V1 video while expanded: body.dataset.phase becomes theater', await waitPhase(page, 'theater', 1000), `phase ${await phase(page)}`);
  await waitClipEnd(page, endsBefore, 2000);
  await waitTransitionsDone(page, 2000);
  await sleep(250);
  const r = await recording(page);
  const grow = r.surfaces.slice(before).find((s) => s.window === 'theater' && s.hit === 'theater');
  check('V2 a surface call { window: theater, hit: theater } is made, and the clip had not yet changed to the theater shape', !!grow && grow.t >= tv && !clipIs(grow.clip, ...THEATER_CLIP), J({ grow, tv }));
  const l = await look(page);
  check('V3 #tvName reads clip.mp4', l.name === 'clip.mp4', J(l));
  check('V4 settled: #theater opacity 1 and #content opacity 0', l.theaterOpacity === '1' && l.contentOpacity === '0', J(l));
  check('V5 #panel clip-path is the theater inset: sides 16 px, bottom 982 - 420 = 562 px, radius 22', clipIs(l.clip, 608, 420), l.clip);
  check('V6 #tv is 576 x 324 at (16, 40) of #theater, radius 12, #0a0a0a', near(l.tv.x, 16) && near(l.tv.y, 40) && near(l.tv.w, 576) && near(l.tv.h, 324) && l.tvRadius === '12px' && l.tvBg === 'rgb(10, 10, 10)', J(l.tv));
  check('V7 #tctl is 576 x 36 at (16, 372) of #theater', near(l.tctl.x, 16) && near(l.tctl.y, 372) && near(l.tctl.w, 576) && near(l.tctl.h, 36), J(l.tctl));
  check('V8 #theater sits at x 16 of the 640 wide stage', near(l.theaterLeft, 16), J(l.theaterLeft));
  check('V9 #vPlay is disabled and #vClose has aria-label "Close video"', l.playDisabled === true && l.closeLabel === 'Close video', J(l));
  check('V10 the window is the theater window', (await viewport(page)) === '640x436', viewport(page));

  // Pinned.
  const mark = (await recording(page)).surfaces.length;
  const phases = (await recording(page)).phases.length;
  await fire(page, 'hover', false);
  await sleep(1000);
  const p = await recording(page);
  check('P1 hover false in the stage, then 1000 ms: still theater with no phase change', (await phase(page)) === 'theater' && p.phases.length === phases, J({ phase: await phase(page), phases: p.phases.slice(phases) }));
  check('P2 no surface call other than theater since', p.surfaces.slice(mark).every((s) => s.window === 'theater'), J(p.surfaces.slice(mark)));
  await fire(page, 'hover', true);
  await fire(page, 'hover', false);
  await sleep(400);
  check('P3 hover in and out again changes nothing either', (await phase(page)) === 'theater' && (await recording(page)).phases.length === phases, await phase(page));

  // Close with a real click, hover false.
  const tail = (await recording(page));
  const base = { phases: tail.phases.length, surfaces: tail.surfaces.length, ends: tail.clipEnds.length, clicks: tail.clicks.length };
  await page.click('#vClose', { timeout: 2000 });
  const c1 = await recording(page);
  const click = c1.clicks[base.clicks];
  const exp = c1.phases.slice(base.phases)[0];
  check('C1 a click on #vClose: the phase is expanded at once', !!exp && exp.phase === 'expanded' && exp.t - click < 100, J({ exp, click }));
  check('C2 the close button takes no focus and calls no focus command', !c1.invokes.some((i) => i.cmd === 'focus') && (await page.evaluate(() => document.activeElement && document.activeElement.id)) !== 'vClose', J(c1.invokes.filter((i) => i.cmd === 'focus')));
  const collapsed = await waitPhase(page, 'collapsed', 4000);
  await sleep(100);
  const c = await recording(page);
  const calls = c.surfaces.slice(base.surfaces), ends = c.clipEnds.slice(base.ends).filter((t) => t > click);
  const afterPhases = c.phases.slice(base.phases);
  const collapsing = afterPhases.find((x) => x.phase === 'collapsing');
  check('C3 the surface calls go { theater, open } first and then { open, open }', calls.length >= 2 && calls[0].window === 'theater' && calls[0].hit === 'open' && calls[1].window === 'open' && calls[1].hit === 'open', J(calls));
  const endAt = ends.length ? ends[0] : null;
  check('C4 { open, open } comes only after the clip\'s transition ended, about 366 ms after the click', !!calls[1] && endAt !== null && calls[1].t >= endAt && endAt - click >= 340, J({ open: calls[1] && calls[1].t - click, end: endAt && endAt - click }));
  check('C5 with hover false the panel then collapses to collapsed and the window pill, the collapse starting 250 ms after the window was open', collapsed && !!collapsing && !!calls[1] && collapsing.t >= calls[1].t + 245 && calls[calls.length - 1].window === 'pill' && viewport(page) === '305x32', J({ collapsed, collapsing: collapsing && collapsing.t - click, open: calls[1] && calls[1].t - click, last: calls[calls.length - 1], vp: viewport(page) }));
  check('C6 no surface call changes the window to pill before the window was open', calls.findIndex((s) => s.window === 'pill') > calls.findIndex((s) => s.window === 'open'), J(calls));
  await page.close();
}

async function scenarioCloseHovered() {
  const page = await openPage({ tag: 'close-hover' });
  await openPanel(page);
  await dropVideo(page);
  const base = await recording(page);
  await page.click('#vClose', { timeout: 2000 });
  await sleep(1000);
  const r = await recording(page);
  const calls = r.surfaces.slice(base.surfaces.length), last = calls[calls.length - 1];
  check('H1 close with hover true: stays expanded after 1000 ms, no collapse started', (await phase(page)) === 'expanded' && !r.phases.slice(base.phases.length).some((p) => p.phase === 'collapsing'), J({ phase: await phase(page), phases: r.phases.slice(base.phases.length) }));
  check('H2 and the window is open', !!last && last.window === 'open' && last.hit === 'open' && viewport(page) === '452x396', J({ last, vp: viewport(page) }));
  await page.close();
}

async function scenarioEscape() {
  const page = await openPage({ tag: 'escape' });
  await openPanel(page);
  await dropVideo(page);
  await fire(page, 'hover', false);
  await sleep(300);
  const base = await recording(page);
  await page.keyboard.press('Escape');
  const r1 = await recording(page);
  const exp = r1.phases.slice(base.phases.length)[0];
  check('X1 Esc in the stage: the phase is expanded at once', !!exp && exp.phase === 'expanded', J(r1.phases.slice(base.phases.length)));
  const collapsed = await waitPhase(page, 'collapsed', 4000);
  await sleep(100);
  const r = await recording(page);
  const calls = r.surfaces.slice(base.surfaces.length), ends = r.clipEnds.slice(base.clipEnds.length);
  const collapsing = r.phases.slice(base.phases.length).find((x) => x.phase === 'collapsing');
  check('X2 the surface calls go { theater, open }, then { open, open } after the clip\'s transition ended', calls.length >= 2 && calls[0].window === 'theater' && calls[0].hit === 'open' && calls[1].window === 'open' && ends.length > 0 && calls[1].t >= ends[0], J({ calls, ends }));
  check('X3 it then collapses to the pill, no collapse before the window was open', collapsed && !!collapsing && !!calls[1] && collapsing.t >= calls[1].t && calls[calls.length - 1].window === 'pill', J({ collapsed, collapsing, calls }));
  await page.close();

  // Esc outside the stage does nothing to the panel.
  const idle = await openPage({ tag: 'escape-open' });
  await openPanel(idle);
  await idle.keyboard.press('Escape');
  await sleep(300);
  check('X4 Esc while expanded (no stage) leaves the panel as it is', (await phase(idle)) === 'expanded', await phase(idle));
  await idle.close();
}

async function scenarioIgnored() {
  const page = await openPage({ tag: 'ignored' });
  await openPanel(page);
  await dropVideo(page);
  const base = await recording(page);
  await fire(page, 'drag', true);
  await sleep(300);
  let l = await look(page);
  check('I1 drag true in the stage changes nothing: the phase stays theater and #drop is not on', l.phase === 'theater' && !l.dropOn, J(l));
  await fire(page, 'video-rejected', 'not an mp4, m4v or mov file');
  await sleep(300);
  l = await look(page);
  check('I2 video-rejected in the stage changes nothing either', l.phase === 'theater' && !l.dropOn && l.dropSub === 'MP4, M4V or MOV', J(l));
  await fire(page, 'drag', false);
  await fire(page, 'hover', false);
  await sleep(500);
  const r = await recording(page);
  check('I3 no phase change and no surface call other than theater followed', r.phases.length === base.phases.length && r.surfaces.slice(base.surfaces.length).every((s) => s.window === 'theater'), J({ phases: r.phases.slice(base.phases.length), surfaces: r.surfaces.slice(base.surfaces.length) }));
  await fire(page, 'video', { path: '/Users/lem/Movies/other.mov', name: 'other.mov' });
  await sleep(200);
  check('I4 a second video replaces the name in the stage', (await look(page)).name === 'other.mov' && (await phase(page)) === 'theater', J(await look(page)));
  await page.close();
}

async function scenarioReduced() {
  const page = await openPage({ tag: 'reduced', reduced: true });
  check('M0 opens the panel', await openPanel(page), `phase ${await phase(page)}`);
  const base = await recording(page);
  const ok = await dropVideo(page, true);
  const r = await recording(page);
  const l = await look(page);
  check('M1 reduced motion: video reaches theater with the clip set at once, no clip-path transition', ok && r.clipRuns.length === base.clipRuns.length && clipIs(l.clip, 608, 420) && l.name === 'clip.mp4', J({ ok, runs: r.clipRuns.length - base.clipRuns.length, clip: l.clip, name: l.name }));
  const grow = r.surfaces.slice(base.surfaces.length).find((s) => s.window === 'theater');
  check('M2 the window grew to theater first', !!grow && !clipIs(grow.clip, ...THEATER_CLIP), J(grow));
  await fire(page, 'hover', false);
  await sleep(200);
  const mark = await recording(page);
  await page.click('#vClose', { timeout: 2000 });
  await sleep(300);
  const c = await recording(page);
  const click = c.clicks[mark.clicks.length];
  const calls = c.surfaces.slice(mark.surfaces.length);
  check('M3 close sends window open at once, with no clip-path transition', calls.length >= 1 && calls[0].window === 'open' && calls[0].hit === 'open' && calls[0].t - click < 250 && c.clipRuns.length === mark.clipRuns.length, J({ calls, click, runs: c.clipRuns.length - mark.clipRuns.length }));
  await page.close();
}

for (const s of [scenarioDrag, scenarioRejected, scenarioStage, scenarioCloseHovered, scenarioEscape, scenarioIgnored, scenarioReduced]) {
  try { await s(); } catch (e) { check(`${s.name} ran`, false, e.stack || String(e)); }
}
await browser.close();
check('E0 no page errors or console errors', pageErrors.length === 0, J(pageErrors));

let failed = 0;
for (const r of results) {
  if (r.ok) console.log(`ok   ${r.name}`);
  else { failed += 1; console.log(`FAIL ${r.name}: ${r.detail}`); }
}
console.log(failed ? `${failed} of ${results.length} theater checks failed` : `all ${results.length} theater checks passed`);
process.exit(failed ? 1 : 0);

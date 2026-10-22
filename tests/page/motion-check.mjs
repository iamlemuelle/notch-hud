// Loads dist/index.html in headless Chromium with a fake Tauri bridge and checks CONTRACT.md section 8 (panel motion,
// banners, the grow-then-shrink window) and the hover rules of section 3. Real time, no retries: phase timers are
// compared with a tolerance of about 40 ms, and every number comes from the contract text, not from the page.
//
// The fake bridge answers `layout`, `media`, `sessions`, `act` and `surface`. `surface` is recorded with the page's
// clock and then resizes the viewport to the contract's `window_size` of the requested mode, standing in for Rust.
// Everything the page does is read back through `document.body.dataset.phase`, element boxes, and a recorder of
// transition events (`transitionrun`/`transitionend`) that keeps each transition's property, duration, delay, easing
// and target value.
//
// Env: PLAYWRIGHT_CORE (path to playwright-core), CHROMIUM (path to a headless chromium), NOTCH_PAGE (page to load,
// default dist/index.html). Exit 0 when every check passes, 1 when one fails, 2 when the browser cannot start.
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

// ---------------------------------------------------------------------------------------------------------------
// Contract numbers (section 8). A 185 x 32 notch, a 982 pt screen, one row, a track showing (media extra 146).
const LAYOUT = {
  stage: { width: 452, height: 982 },
  pill: { width: 305, height: 32 },
  peek: { width: 309, height: 36 },
  banner: { width: 380, height: 78 },
  open: { width: 420, height: 226 },
  ear: 60,
};
const WINDOW = { pill: { w: 305, h: 32 }, banner: { w: 412, h: 94 }, open: { w: 452, h: 242 } };
const SHAPE = { pill: [305, 32, 14], peek: [309, 36, 14], banner: [380, 78, 20], open: [420, 226, 22] };
const STAGE = { w: 452, h: 982 };
const EASE_PEEK = 'cubic-bezier(0.2,0,0,1)';
const EASE_CLOSE = 'cubic-bezier(0.4,0,1,1)';
const AMBER = 'rgb(245, 166, 35)';
const GRAY = 'rgb(139, 139, 139)';

// The spring of the contract: stiffness 380, damping 32, mass 1.
const SPRING = (() => {
  const k = 380, c = 32, m = 1;
  const w0 = Math.sqrt(k / m), z = c / (2 * Math.sqrt(k * m)), a = z * w0, wd = w0 * Math.sqrt(1 - z * z);
  const D = Math.log(Math.sqrt(1 + (a / wd) ** 2) / 0.005) / a;
  const x = (t) => 1 - Math.exp(-a * t) * (Math.cos(wd * t) + (a / wd) * Math.sin(wd * t));
  const points = Array.from({ length: 40 }, (_, i) => (i === 39 ? 1 : Number(x((i / 39) * D).toFixed(4))));
  return { points, ms: Math.round(D * 1000) };
})();

// ---------------------------------------------------------------------------------------------------------------
const results = [];
const check = (name, ok, detail) => results.push({ name, ok: !!ok, detail: ok ? undefined : detail });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const near = (v, target, tol) => typeof v === 'number' && Math.abs(v - target) <= tol;
const J = (v) => JSON.stringify(v);
const norm = (s) => String(s ?? '').replace(/\s+/g, '');
const pageErrors = [];
const seenProps = new Set();

// Installed before any page script: the fake bridge, the recorders and the viewport emulation hook.
function installBridge({ layout, windows, holdLayout, initial }) {
  const rec = (window.__rec = { events: [], surfaces: [], phases: [], trans: [], resizes: [], invokes: [] });
  const handlers = {};
  const now = () => performance.now();
  window.__fire = (name, payload) => {
    const t = now();
    rec.events.push({ name, t });
    (handlers[name] || []).forEach((h) => h({ payload }));
    return t;
  };
  let releaseLayout;
  window.__releaseLayout = () => releaseLayout && releaseLayout();
  // `surface` resolves after `__surfaceDelay` ms and after the viewport took the requested size, like Rust placing the
  // window; a call whose "<window> <hit>" key is in `__reject` rejects once instead.
  window.__surfaceDelay = 0;
  window.__reject = [];
  const late = (value) => new Promise((r) => setTimeout(() => r(value), (initial && initial.delay) || 0));
  window.__TAURI__ = {
    event: { listen: (name, h) => { (handlers[name] = handlers[name] || []).push(h); return Promise.resolve(() => {}); } },
    core: {
      invoke: (cmd, args) => {
        rec.invokes.push({ cmd, t: now() });
        if (cmd === 'layout') {
          if (holdLayout) return new Promise((r) => { releaseLayout = () => r(layout); });
          return Promise.resolve(layout);
        }
        if (cmd === 'media') return late(initial ? initial.media : null);
        if (cmd === 'sessions') return late(initial ? initial.sessions : null);
        if (cmd === 'surface') {
          const entry = { t: now(), args: JSON.parse(JSON.stringify(args ?? null)), resolved: null, rejected: false };
          rec.surfaces.push(entry);
          const at = window.__reject.indexOf(args && args.window + ' ' + args.hit);
          if (at >= 0) {
            window.__reject.splice(at, 1);
            entry.rejected = true;
            return Promise.reject(new Error('surface rejected by the mock'));
          }
          const w = args && windows[args.window];
          return new Promise((resolve) => setTimeout(async () => {
            if (w) await window.__setWindow(w.w, w.h);
            entry.resolved = now();
            resolve(null);
          }, window.__surfaceDelay));
        }
        return Promise.resolve(null);
      },
    },
  };
  const mk = (w, h) => { const c = document.createElement('canvas'); c.width = w; c.height = h; const x = c.getContext('2d'); x.fillStyle = '#4f7cff'; x.fillRect(0, 0, w, h); return c.toDataURL(); };
  window.__art = { square: mk(256, 256), wide: mk(320, 180) };

  // Phase log: one entry per data-phase change, reconstructed from old values so same-task changes are not lost.
  const phaseNow = () => (document.body && document.body.dataset.phase) || null;
  new MutationObserver((ms) => {
    const t = now();
    ms.forEach((m, i) => {
      const value = i + 1 < ms.length ? ms[i + 1].oldValue : phaseNow();
      rec.phases.push({ phase: value, t });
    });
  }).observe(document, { attributes: true, attributeFilter: ['data-phase'], attributeOldValue: true, subtree: true });
  document.addEventListener('DOMContentLoaded', () => { if (phaseNow()) rec.phases.push({ phase: phaseNow(), t: now() }); });

  window.addEventListener('resize', () => rec.resizes.push({ t: now(), w: innerWidth, h: innerHeight }));

  // Transition log.
  const camel = (p) => p.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
  const sel = (e) => { const n = e.target; return (n.id ? '#' + n.id : String(n.tagName || n.nodeName).toLowerCase()) + (e.pseudoElement || ''); };
  const open = (e) => rec.trans.slice().reverse().find((r) => r.sel === sel(e) && r.prop === e.propertyName && r.end == null && r.cancel == null);
  document.addEventListener('transitionrun', (e) => {
    const a = e.target.getAnimations().filter((x) => x.transitionProperty === e.propertyName).pop();
    let timing = null, from = null, to = null;
    if (a) {
      const tm = a.effect.getTiming();
      timing = { duration: tm.duration, delay: tm.delay, easing: tm.easing };
      const kf = a.effect.getKeyframes();
      from = kf[0] && kf[0][camel(e.propertyName)];
      to = kf[kf.length - 1] && kf[kf.length - 1][camel(e.propertyName)];
    }
    rec.trans.push({ sel: sel(e), prop: e.propertyName, run: now(), vw: innerWidth, vh: innerHeight, start: null, end: null, cancel: null, timing, from, to });
  }, true);
  document.addEventListener('transitionstart', (e) => { const r = open(e); if (r) r.start = now(); }, true);
  document.addEventListener('transitionend', (e) => { const r = open(e); if (r) r.end = now(); }, true);
  document.addEventListener('transitioncancel', (e) => { const r = open(e); if (r) r.cancel = now(); }, true);

  // Every property that is animated (CSS transitions and keyframe animations) at this moment.
  window.__animatedProps = () => {
    const out = new Set();
    for (const a of document.getAnimations()) {
      if (a.transitionProperty) out.add(a.transitionProperty);
      const kf = a.effect && a.effect.getKeyframes ? a.effect.getKeyframes() : [];
      kf.forEach((k) => Object.keys(k).forEach((p) => { if (!['offset', 'computedOffset', 'easing', 'composite'].includes(p)) out.add(p.replace(/[A-Z]/g, (c) => '-' + c.toLowerCase())); }));
    }
    return [...out];
  };

  // A snapshot of the page: phase, viewport, the stage box, and for each selector its box relative to the stage.
  window.__snap = (selectors) => {
    const stage = document.querySelector('#stage');
    const sr = stage ? stage.getBoundingClientRect() : null;
    const out = { phase: phaseNow(), vw: innerWidth, vh: innerHeight, stage: sr && { x: sr.left, y: sr.top, w: sr.width, h: sr.height, cx: sr.left + sr.width / 2 }, el: {} };
    for (const s of selectors) {
      const e = document.querySelector(s);
      if (!e) { out.el[s] = null; continue; }
      const r = e.getBoundingClientRect();
      const cs = getComputedStyle(e);
      let eff = 1;
      for (let n = e; n && n.nodeType === 1; n = n.parentElement) {
        const c = getComputedStyle(n);
        if (c.display === 'none' || c.visibility === 'hidden') { eff = 0; break; }
        eff *= parseFloat(c.opacity);
      }
      out.el[s] = {
        x: r.left - (sr ? sr.left : 0), y: r.top - (sr ? sr.top : 0), w: r.width, h: r.height,
        opacity: parseFloat(cs.opacity), eff: r.width > 0 && r.height > 0 ? eff : 0,
        text: e.textContent.trim(), clip: cs.clipPath, transform: cs.transform,
        colors: [e, ...e.querySelectorAll('*')].map((n) => getComputedStyle(n).backgroundColor),
      };
    }
    return out;
  };
}

const ELEMENTS = ['#stage', '#panel', '#earL', '#warn', '#earBars', '#earPause', '#art', '#content', '#banner', '#bTitle', '#bSub', '#bIcon', '#bannerBars'];

async function openPage(opts = {}) {
  const page = await browser.newPage({ viewport: { width: WINDOW.pill.w, height: WINDOW.pill.h } });
  const tag = opts.tag || 'page';
  page.on('pageerror', (e) => pageErrors.push(`${tag}: pageerror: ${e.message}`));
  page.on('console', (m) => { if (m.type() === 'error') pageErrors.push(`${tag}: console: ${m.text()}`); });
  if (opts.reduced) await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.exposeFunction('__setWindow', (w, h) => page.setViewportSize({ width: w, height: h }).catch(() => {}));
  await page.addInitScript(installBridge, { layout: LAYOUT, windows: WINDOW, holdLayout: !!opts.holdLayout, initial: opts.initial || null });
  await page.goto('file://' + PAGE_FILE);
  if (!opts.holdLayout) await page.waitForFunction(() => document.body.dataset.phase, null, { timeout: 1500 }).catch(() => {});
  return page;
}

const fire = (page, name, payload) => page.evaluate(([n, p]) => window.__fire(n, p), [name, payload]);
const rec = async (page) => {
  const r = await page.evaluate(() => JSON.parse(JSON.stringify(window.__rec)));
  r.trans.forEach((x) => seenProps.add(x.prop));
  return r;
};
const snap = (page, selectors = ELEMENTS) => page.evaluate((s) => window.__snap(s), selectors);
const art = (page, kind) => page.evaluate((k) => window.__art[k], kind);
const collectProps = async (page) => { (await page.evaluate(() => window.__animatedProps())).forEach((p) => seenProps.add(p)); };
const waitPhase = (page, phase, ms) => page.waitForFunction((p) => document.body.dataset.phase === p, phase, { timeout: ms, polling: 20 }).then(() => true, () => false);

const track = (art, over = {}) => ({ title: 'Low Tide Static', artist: 'Mara Venn', album: 'Fixture Album', app: 'Apple Music', bundle_id: 'com.apple.Music', playing: true, duration_ms: 223000, elapsed_ms: 74000, elapsed_at_ms: Date.now(), artwork: art, ...over });
const media = (t) => ({ via: 'adapter', health: 'ok', track: t });
const row = (label, status, detail = null) => ({ label, status, detail });
const summary = (rows) => ({
  rows, total: rows.length,
  attention: rows.filter((r) => r.status === 'needs_permission' || r.status === 'needs_input').length,
  working: rows.filter((r) => r.status === 'working').length,
});

// ---- readers over a recording ------------------------------------------------------------------------------------
const phaseAt = (r, name, from, to = Infinity) => r.phases.find((p) => p.phase === name && p.t >= from - 2 && p.t <= to);
const surfaceCalls = (r, from = -Infinity, to = Infinity) => r.surfaces.filter((s) => s.t >= from - 2 && s.t <= to);
const notPill = (s) => !(s.args && s.args.window === 'pill' && s.args.hit === 'pill');
const surfaceIs = (s, win, hit) => s && J(Object.keys(s.args || {}).sort()) === J(['hit', 'window']) && s.args.window === win && s.args.hit === hit;
const trans = (r, sel, prop, from, to = Infinity) => r.trans.filter((x) => x.sel === sel && x.prop === prop && x.run >= from - 2 && x.run <= to);
const timingIs = (x, duration, delay, easing) => !!x && !!x.timing && x.timing.duration === duration && x.timing.delay === delay && norm(x.timing.easing) === norm(easing);

// A CSS linear() easing as the list of its output values, or null.
function linearPoints(easing) {
  const m = /^linear\((.*)\)$/s.exec(String(easing ?? '').trim());
  if (!m) return null;
  return m[1].split(',').map((p) => parseFloat(p.trim().split(/\s+/)[0]));
}
function isSpring(x) {
  if (!x || !x.timing || x.timing.duration !== SPRING.ms || x.timing.delay !== 0) return `timing ${J(x && x.timing)}`;
  const pts = linearPoints(x.timing.easing);
  if (!pts) return `easing is not linear(): ${x.timing.easing}`;
  if (pts.length !== 40) return `linear() has ${pts.length} points, want 40`;
  const bad = pts.findIndex((v, i) => !(Math.abs(v - SPRING.points[i]) < 1e-6));
  if (bad >= 0) return `point ${bad} is ${pts[bad]}, want ${SPRING.points[bad]}`;
  return null;
}

// "inset(0px 73.5px 950px round 0px 0px 14px 14px)" to the visible size, the top radii and the bottom radii.
function parseClip(s) {
  const m = /^inset\((.+?)(?:\s+round\s+(.+))?\)$/.exec(String(s ?? '').trim());
  if (!m) return null;
  const four = (list, pattern) => {
    const v = list.trim().split(/\s+/).map(parseFloat);
    if (v.some(Number.isNaN)) return null;
    return v.length === 1 ? [v[0], v[0], v[0], v[0]] : v.length === 2 ? [v[0], v[1], v[0], v[1]] : v.length === 3 ? [v[0], v[1], v[2], v[1]] : v.length === 4 ? v : null;
  };
  const edges = four(m[1]);
  if (!edges) return null;
  const [top, right, bottom, left] = edges;
  const radii = m[2] ? four(m[2]) : [0, 0, 0, 0];
  return radii ? { w: STAGE.w - left - right, h: STAGE.h - top - bottom, topRadius: Math.max(radii[0], radii[1]), bottomRadius: [radii[2], radii[3]] } : null;
}
function clipIs(s, shape) {
  const c = parseClip(s);
  const [w, h, r] = SHAPE[shape];
  const ok = c && near(c.w, w, 0.6) && near(c.h, h, 0.6) && c.topRadius === 0 && c.bottomRadius.every((x) => near(x, r, 0.1));
  return ok ? null : `clip ${s} is not the ${shape} shape ${w}x${h} radius ${r}`;
}

const visible = (e) => !!e && e.eff > 0.01;
const hidden = (e) => !e || e.eff <= 0.01;
const boxIs = (e, x, y, w, h, tol = 0.6) => !!e && near(e.x, x, tol) && near(e.y, y, tol) && near(e.w, w, tol) && near(e.h, h, tol);
const explain = (e) => (e ? `${J({ x: e.x, y: e.y, w: e.w, h: e.h, eff: e.eff, op: e.opacity })}` : 'missing');

// The stage keeps its place on screen: its center is the viewport's center, its top is the viewport's top.
function stageStays(s) {
  return !!s.stage && near(s.stage.cx, s.vw / 2, 0.6) && near(s.stage.y, 0, 0.6) && near(s.stage.w, STAGE.w, 0.6) && near(s.stage.h, STAGE.h, 0.6);
}

// Scenarios ------------------------------------------------------------------------------------------------------

// Stage hidden until the first `layout` arrives.
async function scenarioLayout() {
  const page = await openPage({ tag: 'layout', holdLayout: true });
  await sleep(300);
  const before = await snap(page, ['#stage', '#panel']);
  check('L1 stage hidden until the first layout arrives', hidden(before.el['#stage']) || hidden(before.el['#panel']), explain(before.el['#stage']));
  await page.evaluate(() => { window.__releaseLayout(); });
  await fire(page, 'layout', LAYOUT);
  await sleep(400);
  const after = await snap(page, ['#stage', '#panel']);
  check('L2 stage is the layout size, centered on the window, once layout arrives', visible(after.el['#stage']) && stageStays(after) && after.phase === 'collapsed', J({ phase: after.phase, stage: after.stage }));
  await page.close();
}

// Hover in and out: the timeline of the phase table.
async function scenarioHover() {
  const page = await openPage({ tag: 'hover' });
  const sq = await art(page, 'square');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(sq)));
  await sleep(700);
  const rest = await snap(page);
  check('H0 collapsed at rest: phase, stage, clip, ear art, ears at 0.82, window pill', rest.phase === 'collapsed' && stageStays(rest) && rest.vw === 305 && rest.vh === 32 && boxIs(rest.el['#panel'], 0, 0, STAGE.w, STAGE.h) && !clipIs(rest.el['#panel'] && rest.el['#panel'].clip, 'pill') && near(rest.el['#earL'] && rest.el['#earL'].opacity, 0.82, 0.01) && near(rest.el['#warn'] && rest.el['#warn'].opacity, 0.82, 0.01),
    J({ phase: rest.phase, vw: rest.vw, vh: rest.vh, stage: rest.stage, panel: explain(rest.el['#panel']), clip: rest.el['#panel'] && clipIs(rest.el['#panel'].clip, 'pill'), earL: explain(rest.el['#earL']), warn: explain(rest.el['#warn']) }));
  check('H0a collapsed #art is a square 18x18 at stage (326.5, 7)', boxIs(rest.el['#art'], 326.5, 7, 18, 18), explain(rest.el['#art']));
  check('H0b nothing but the resting pill is sent to surface before any hover', (await rec(page)).surfaces.every((s) => !notPill(s)), J((await rec(page)).surfaces));
  await collectProps(page);

  // A: hover in, leave before 80 ms.
  const [tIn, tOut] = await page.evaluate(async () => { const a = window.__fire('hover', true); await new Promise((r) => setTimeout(r, 40)); const b = window.__fire('hover', false); return [a, b]; });
  await sleep(350);
  const ra = await rec(page);
  const intent = phaseAt(ra, 'intent', tIn, tIn + 30);
  check('H1 hover in goes to intent', !!intent, J(ra.phases.filter((p) => p.t >= tIn)));
  const back = phaseAt(ra, 'collapsed', tOut, tOut + 30);
  check('H2 leaving before 80 ms goes back to collapsed, no peek, no open window', !!back && !ra.phases.some((p) => p.t >= tIn && p.phase === 'peek') && surfaceCalls(ra, tIn).every((s) => s.args && s.args.window !== 'open'),
    J({ phases: ra.phases.filter((p) => p.t >= tIn), surfaces: surfaceCalls(ra, tIn) }));

  // B: hover held, then settled.
  const t0 = await fire(page, 'hover', true);
  await sleep(950);
  const open = await snap(page);
  const rb = await rec(page);
  const peek = phaseAt(rb, 'peek', t0, t0 + 200);
  check('H3 peek at about 80 ms', !!peek && near(peek.t - t0, 80, 40), J({ peek: peek && peek.t - t0, phases: rb.phases.filter((p) => p.t >= t0).map((p) => [p.phase, Math.round(p.t - t0)]) }));
  const peekClip = trans(rb, '#panel', 'clip-path', t0, t0 + 200)[0];
  check('H4 peek: #panel clip-path transition 120 ms cubic-bezier(0.2, 0, 0, 1), to the peek shape', timingIs(peekClip, 120, 0, EASE_PEEK) && !clipIs(peekClip.to, 'peek') && peek && near(peekClip.run - peek.t, 0, 40),
    J({ peekClip, clip: peekClip && clipIs(peekClip.to, 'peek') }));
  const sPeek = surfaceCalls(rb, t0).find((s) => surfaceIs(s, 'open', 'peek'));
  check('H5 surface {window: "open", hit: "peek"} is sent when peek begins', !!sPeek && !!peek && near(sPeek.t, peek.t, 40), J({ sPeek, peek, all: surfaceCalls(rb, t0) }));
  const exp = phaseAt(rb, 'expanded', t0, t0 + 400);
  check('H6 expanded at about 200 ms', !!exp && near(exp.t - t0, 200, 40), J({ exp: exp && exp.t - t0 }));
  const expClip = trans(rb, '#panel', 'clip-path', t0 + 150, t0 + 300)[0];
  check('H7 expand: #panel clip-path transition 366 ms, linear() of the 40-point spring, to the open shape', !!expClip && !isSpring(expClip) && !clipIs(expClip.to, 'open'), J({ spring: expClip && isSpring(expClip), clip: expClip && clipIs(expClip.to, 'open'), timing: expClip && expClip.timing }));
  check('H8 surface {window: "open", hit: "open"} is sent on expanding', surfaceCalls(rb, t0).some((s) => surfaceIs(s, 'open', 'open') && exp && near(s.t, exp.t, 40)), J(surfaceCalls(rb, t0)));
  const cOp = trans(rb, '#content', 'opacity', t0 + 150, t0 + 300)[0], cTf = trans(rb, '#content', 'transform', t0 + 150, t0 + 300)[0];
  check('H9 #content opacity and transform: 140 ms ease-out, delay 80 ms, from 0 and 4 px down', timingIs(cOp, 140, 80, 'ease-out') && timingIs(cTf, 140, 80, 'ease-out') && Number(cOp.from) === 0 && Number(cOp.to) === 1 && /translateY\(4px\)|matrix\(1, 0, 0, 1, 0, 4\)/.test(cTf.from || ''),
    J({ cOp, cTf }));
  const aTf = trans(rb, '#art', 'transform', t0 + 150, t0 + 300)[0];
  check('H10 #art transform: 366 ms, the same linear() spring', !!aTf && !isSpring(aTf), J({ spring: aTf && isSpring(aTf), aTf }));
  check('H11 window grows: the viewport is the open window 452x242', open.vw === 452 && open.vh === 242, J({ vw: open.vw, vh: open.vh }));
  check('H12 expanded geometry: stage keeps its place, clip is the open shape, ears at 1', open.phase === 'expanded' && stageStays(open) && !clipIs(open.el['#panel'] && open.el['#panel'].clip, 'open') && near(open.el['#earL'] && open.el['#earL'].opacity, 1, 0.01) && near(open.el['#warn'] && open.el['#warn'].opacity, 1, 0.01),
    J({ phase: open.phase, stage: open.stage, vw: open.vw, clip: open.el['#panel'] && clipIs(open.el['#panel'].clip, 'open'), earL: explain(open.el['#earL']) }));
  check('H13 expanded #art is a square 64x64 at stage (34, 44)', boxIs(open.el['#art'], 34, 44, 64, 64), explain(open.el['#art']));
  check('H14 expanded: #content shown, ear art gone from the ear', visible(open.el['#content']) && near(open.el['#content'].opacity, 1, 0.01), explain(open.el['#content']));
  await collectProps(page);

  // C: hover out while expanded.
  const t1 = await fire(page, 'hover', false);
  await sleep(1100);
  const rc = await rec(page);
  const early = rc.phases.filter((p) => p.t > t1 + 2 && p.t < t1 + 210);
  check('H15 hover out: still expanded at about 200 ms', early.length === 0, J(early));
  const col = phaseAt(rc, 'collapsing', t1, t1 + 400);
  check('H16 collapsing at about 250 ms', !!col && near(col.t - t1, 250, 40), J({ col: col && col.t - t1 }));
  const fade = trans(rc, '#content', 'opacity', t1 + 150, t1 + 400)[0];
  check('H17 #content opacity fades out in 80 ms linear when collapsing begins', timingIs(fade, 80, 0, 'linear') && Number(fade.to) === 0 && col && near(fade.run - col.t, 0, 40), J({ fade }));
  const closeClip = trans(rc, '#panel', 'clip-path', t1 + 200, t1 + 500)[0];
  check('H18 close: clip-path 200 ms cubic-bezier(0.4, 0, 1, 1) at about 330 ms, to the pill shape', timingIs(closeClip, 200, 0, EASE_CLOSE) && !clipIs(closeClip.to, 'pill') && near(closeClip.run - t1, 330, 40), J({ closeClip, at: closeClip && closeClip.run - t1, clip: closeClip && clipIs(closeClip.to, 'pill') }));
  const closeArt = trans(rc, '#art', 'transform', t1 + 200, t1 + 500)[0];
  check('H19 close: #art transform 200 ms cubic-bezier(0.4, 0, 1, 1) back to the ear', timingIs(closeArt, 200, 0, EASE_CLOSE), J({ closeArt }));
  const done = phaseAt(rc, 'collapsed', t1 + 300, t1 + 800);
  check('H20 collapsed at about 530 ms', !!done && near(done.t - t1, 530, 40), J({ done: done && done.t - t1 }));
  const pillCalls = surfaceCalls(rc, t1).filter((s) => s.args && s.args.window === 'pill');
  const endOfClip = closeClip && closeClip.end;
  check('H21 surface {window: "pill", hit: "pill"} only after the clip transition ended', pillCalls.length >= 1 && surfaceIs(pillCalls[0], 'pill', 'pill') && endOfClip != null && pillCalls[0].t >= endOfClip - 2 && pillCalls[0].t <= endOfClip + 80,
    J({ pillCalls: pillCalls.map((s) => [s.args, Math.round(s.t - t1)]), clipEnd: endOfClip && Math.round(endOfClip - t1), clipCancel: closeClip && closeClip.cancel }));
  const after = await snap(page);
  check('H22 back to the pill window 305x32, stage in place, clip is the pill, ears dimmed', after.vw === 305 && after.vh === 32 && after.phase === 'collapsed' && stageStays(after) && !clipIs(after.el['#panel'] && after.el['#panel'].clip, 'pill') && near(after.el['#earL'] && after.el['#earL'].opacity, 0.82, 0.01),
    J({ vw: after.vw, vh: after.vh, phase: after.phase, clip: after.el['#panel'] && clipIs(after.el['#panel'].clip, 'pill'), earL: explain(after.el['#earL']) }));
  check('H23 collapsed again: #art is back in the ear at (326.5, 7) 18x18', boxIs(after.el['#art'], 326.5, 7, 18, 18), explain(after.el['#art']));

  // D: hover in again during collapsing cancels the shrink.
  const u0 = await fire(page, 'hover', true);
  await sleep(950);
  const u1 = await fire(page, 'hover', false);
  await sleep(400);
  const u2 = await fire(page, 'hover', true);
  await sleep(950);
  const rd = await rec(page);
  const endState = await snap(page);
  const colAgain = phaseAt(rd, 'collapsing', u1, u2);
  const intent2 = phaseAt(rd, 'intent', u2, u2 + 30);
  const peek2 = phaseAt(rd, 'peek', u2, u2 + 200);
  const exp2 = phaseAt(rd, 'expanded', u2, u2 + 500);
  check('H24 hover in during collapsing: intent, then peek at 80 ms, then expanded at 200 ms', !!colAgain && !!intent2 && !!peek2 && !!exp2 && near(peek2.t - u2, 80, 40) && near(exp2.t - u2, 200, 40), J({ colAgain: !!colAgain, phases: rd.phases.filter((p) => p.t >= u1).map((p) => [p.phase, Math.round(p.t - u2)]) }));
  check('H25 hover in during collapsing: no pill window is sent, the window stays open', surfaceCalls(rd, u1).every((s) => !(s.args && s.args.window === 'pill')) && endState.vw === 452 && endState.vh === 242 && !rd.resizes.some((x) => x.t > u1 && x.w === 305), J({ surfaces: surfaceCalls(rd, u1), vw: endState.vw, vh: endState.vh }));
  check('H26 hover in during collapsing: expanded again, clip is the open shape', endState.phase === 'expanded' && !clipIs(endState.el['#panel'] && endState.el['#panel'].clip, 'open'), J({ phase: endState.phase }));
  await fire(page, 'hover', false);
  await sleep(300);
  await page.close();
}

// Banners from a song change.
async function scenarioSongBanner() {
  const page = await openPage({ tag: 'song-banner' });
  const sq = await art(page, 'square');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(sq)));
  await sleep(700);
  const r0 = await rec(page);
  check('B1 the first media and the first sessions payloads show no banner', !r0.surfaces.some(notPill) && !r0.phases.some((p) => p.phase === 'banner'), J({ surfaces: r0.surfaces, phases: r0.phases }));
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(sq, { playing: false })));
  await sleep(300);
  const r1 = await rec(page);
  check('B2 the same track, only paused, shows no banner', !r1.phases.some((p) => p.phase === 'banner'), J(r1.phases));

  const tB = await fire(page, 'media', media(track(sq, { title: 'Second Song', playing: true })));
  await sleep(120);
  const early = await rec(page);
  const banner = phaseAt(early, 'banner', tB, tB + 150);
  check('B3 a new title while collapsed goes to banner', !!banner && banner.t - tB < 60, J(early.phases.filter((p) => p.t >= tB)));
  check('B4 surface {window: "banner", hit: "banner"} is sent', surfaceCalls(early, tB).some((s) => surfaceIs(s, 'banner', 'banner')), J(surfaceCalls(early, tB)));
  await sleep(700);
  const s = await snap(page);
  const rs = await rec(page);
  check('B5 window is the banner window 412x94 and the stage keeps its place', s.vw === 412 && s.vh === 94 && stageStays(s), J({ vw: s.vw, vh: s.vh, stage: s.stage }));
  check('B6 #bTitle is the title, #bSub is "<artist>, on <app>"', s.el['#bTitle'] && s.el['#bTitle'].text === 'Second Song' && s.el['#bSub'] && s.el['#bSub'].text === 'Mara Venn, on Apple Music', J({ title: s.el['#bTitle'] && s.el['#bTitle'].text, sub: s.el['#bSub'] && s.el['#bSub'].text }));
  check('B7 banner: #art is a 34x34 square at stage (52, 36), #bIcon hidden, #bannerBars shown at x 380', boxIs(s.el['#art'], 52, 36, 34, 34) && hidden(s.el['#bIcon']) && visible(s.el['#bannerBars']) && near(s.el['#bannerBars'].x, 380, 1.5), J({ art: explain(s.el['#art']), icon: explain(s.el['#bIcon']), bars: explain(s.el['#bannerBars']) }));
  check('B8 banner: clip is the banner shape 380x78', !clipIs(s.el['#panel'] && s.el['#panel'].clip, 'banner'), J({ clip: s.el['#panel'] && clipIs(s.el['#panel'].clip, 'banner') }));
  const bClip = trans(rs, '#panel', 'clip-path', tB, tB + 400)[0], bIn = trans(rs, '#banner', 'opacity', tB, tB + 400)[0], bArt = trans(rs, '#art', 'transform', tB, tB + 400)[0];
  check('B9 banner in: clip and #art spring 366 ms, #banner opacity 140 ms ease-out delay 80 ms', !!bClip && !isSpring(bClip) && !!bArt && !isSpring(bArt) && timingIs(bIn, 140, 80, 'ease-out'), J({ clip: bClip && isSpring(bClip), art: bArt && isSpring(bArt), bIn }));
  await collectProps(page);
  const closed = await waitPhase(page, 'collapsed', 3600);
  const rEnd = await rec(page);
  const collapsedAt = phaseAt(rEnd, 'collapsed', tB + 1000);
  const colAt = phaseAt(rEnd, 'collapsing', tB + 1000);
  check('B10 banner closes after 2600 ms (collapsing), collapsed after 200 + 80 ms more', closed && !!colAt && near(colAt.t - banner.t, 2600, 60) && !!collapsedAt && near(collapsedAt.t - banner.t, 2880, 80), J({ closed, collapsing: colAt && colAt.t - (banner && banner.t), collapsed: collapsedAt && collapsedAt.t - (banner && banner.t) }));
  await sleep(150);
  const rWin = await rec(page);
  const clipClose = trans(rWin, '#panel', 'clip-path', tB + 2000).pop();
  const pillCall = surfaceCalls(rWin, tB + 2000).find((x) => x.args && x.args.window === 'pill');
  check('B11 banner close: window pill only after the clip transition ended', !!pillCall && surfaceIs(pillCall, 'pill', 'pill') && !!clipClose && clipClose.end != null && pillCall.t >= clipClose.end - 2, J({ pillCall, clipClose }));
  const back = await snap(page);
  check('B12 after the banner: pill window 305x32, #art back in the ear at (326.5, 7)', back.vw === 305 && back.vh === 32 && boxIs(back.el['#art'], 326.5, 7, 18, 18), J({ vw: back.vw, vh: back.vh, art: explain(back.el['#art']) }));

  // Hover in during a banner: expanded after 80 ms with the window open.
  const tC = await fire(page, 'media', media(track(sq, { title: 'Third Song' })));
  await sleep(300);
  const tH = await fire(page, 'hover', true);
  await sleep(900);
  const rh = await rec(page);
  const expanded = phaseAt(rh, 'expanded', tH, tH + 400);
  check('B13 hover in during a banner: expanded after about 80 ms', !!expanded && near(expanded.t - tH, 80, 40) && !!phaseAt(rh, 'banner', tC, tH), J(rh.phases.filter((p) => p.t >= tC).map((p) => [p.phase, Math.round(p.t - tH)])));
  const openCall = surfaceCalls(rh, tH).find((x) => surfaceIs(x, 'open', 'open'));
  const hs = await snap(page);
  check('B14 hover in during a banner: surface open/open, window 452x242, clip is the open shape', !!openCall && hs.vw === 452 && hs.vh === 242 && !clipIs(hs.el['#panel'] && hs.el['#panel'].clip, 'open'), J({ openCall, vw: hs.vw, vh: hs.vh }));
  const bOut = trans(rh, '#banner', 'opacity', tH).find((x) => x.timing && x.timing.duration === 80);
  check('B15 the banner layer fades out in 80 ms when expanding from a banner', !!bOut && Number(bOut.to) === 0, J(trans(rh, '#banner', 'opacity', tH)));

  // A change while expanded shows no banner and is not queued.
  const tD = await fire(page, 'media', media(track(sq, { title: 'Fourth Song' })));
  await sleep(400);
  const rd = await rec(page);
  check('B16 a media change while expanded shows no banner', !rd.phases.some((p) => p.t >= tD && p.phase === 'banner'), J(rd.phases.filter((p) => p.t >= tD)));
  const tL = await fire(page, 'hover', false);
  await sleep(1700);
  const re = await rec(page);
  check('B17 the missed banner is not queued: after collapsing the page stays collapsed', !re.phases.some((p) => p.t >= tD && p.phase === 'banner') && (await snap(page, [])).phase === 'collapsed', J(re.phases.filter((p) => p.t >= tD)));
  await page.close();
}

// Wide artwork and a track without an artist.
async function scenarioWideBanner() {
  const page = await openPage({ tag: 'wide-banner' });
  const wide = await art(page, 'wide');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(wide, { title: 'Why your build is slow', artist: 'Compile Time', app: 'Chrome', bundle_id: 'com.google.Chrome' })));
  await sleep(800);
  const rest = await snap(page);
  check('W1 collapsed wide #art is 26x15 at stage (324.5, 8.5)', boxIs(rest.el['#art'], 324.5, 8.5, 26, 15, 0.9), explain(rest.el['#art']));
  const tW = await fire(page, 'media', media(track(wide, { title: 'Second video', artist: 'Compile Time', app: 'Chrome', bundle_id: 'com.google.Chrome' })));
  await sleep(900);
  const s = await snap(page);
  check('W2 wide banner: #bSub is "<artist>, in <app>"', s.phase === 'banner' && s.el['#bSub'] && s.el['#bSub'].text === 'Compile Time, in Chrome', J({ phase: s.phase, sub: s.el['#bSub'] && s.el['#bSub'].text }));
  check('W3 wide banner: #art is 60x34 at stage (52, 36), text at x 124', boxIs(s.el['#art'], 52, 36, 60, 33.75, 0.9) && s.el['#bTitle'] && near(s.el['#bTitle'].x, 36 + 88, 1.5), J({ art: explain(s.el['#art']), title: explain(s.el['#bTitle']) }));
  await waitPhase(page, 'collapsed', 3600);
  await sleep(200);
  await fire(page, 'media', media(track(wide, { title: 'A podcast', artist: null, app: 'Chrome', bundle_id: 'com.google.Chrome' })));
  await sleep(500);
  const n = await snap(page);
  check('W4 no artist: #bSub is the app alone', n.phase === 'banner' && n.el['#bSub'] && n.el['#bSub'].text === 'Chrome' && n.el['#bTitle'].text === 'A podcast', J({ phase: n.phase, sub: n.el['#bSub'] && n.el['#bSub'].text }));
  await waitPhase(page, 'collapsed', 3600);
  await sleep(200);
  await fire(page, 'hover', true);
  await sleep(950);
  const o = await snap(page);
  check('W5 expanded wide #art is 112x63 at stage (34, 44)', boxIs(o.el['#art'], 34, 44, 112, 63), explain(o.el['#art']));
  await fire(page, 'hover', false);
  await sleep(300);
  await page.close();
}

// Banners from session rows.
async function scenarioSessionBanner() {
  const page = await openPage({ tag: 'session-banner' });
  await fire(page, 'sessions', summary([row('paxiv', 'working'), row('rails', 'working')]));
  await sleep(500);
  const r0 = await rec(page);
  check('S1 the first sessions payload shows no banner', !r0.surfaces.some(notPill) && !r0.phases.some((p) => p.phase === 'banner'), J({ surfaces: r0.surfaces }));

  await fire(page, 'sessions', summary([row('paxiv', 'needs_input'), row('rails', 'working')]));
  await sleep(900);
  const a = await snap(page);
  check('S2 working to needs_input: banner with the label, "needs your input", amber dot', a.phase === 'banner' && a.el['#bTitle'].text === 'paxiv' && a.el['#bSub'].text === 'needs your input' && a.el['#bIcon'].colors.includes(AMBER), J({ phase: a.phase, title: a.el['#bTitle'] && a.el['#bTitle'].text, sub: a.el['#bSub'] && a.el['#bSub'].text, colors: a.el['#bIcon'] && a.el['#bIcon'].colors }));
  check('S3 session banner: #bIcon 34x34 at stage (52, 36), no equalizer bars, window 412x94', boxIs(a.el['#bIcon'], 52, 36, 34, 34) && hidden(a.el['#bannerBars']) && a.vw === 412 && a.vh === 94, J({ icon: explain(a.el['#bIcon']), bars: explain(a.el['#bannerBars']), vw: a.vw }));
  await waitPhase(page, 'collapsed', 3600);
  await sleep(250);

  await fire(page, 'sessions', summary([row('paxiv', 'done'), row('rails', 'working')]));
  await sleep(600);
  const d = await snap(page);
  check('S4 needs_input to done: banner "done" with a gray dot', d.phase === 'banner' && d.el['#bTitle'].text === 'paxiv' && d.el['#bSub'].text === 'done' && d.el['#bIcon'].colors.includes(GRAY) && !d.el['#bIcon'].colors.includes(AMBER), J({ phase: d.phase, sub: d.el['#bSub'] && d.el['#bSub'].text, colors: d.el['#bIcon'] && d.el['#bIcon'].colors }));
  await waitPhase(page, 'collapsed', 3600);
  await sleep(250);

  const tNone = await fire(page, 'sessions', summary([row('paxiv', 'working'), row('rails', 'working')]));
  await sleep(250);
  await fire(page, 'sessions', summary([row('paxiv', 'waiting'), row('rails', 'working')]));
  await sleep(250);
  const rn = await rec(page);
  check('S5 changes to working or waiting show no banner', !rn.phases.some((p) => p.t >= tNone && p.phase === 'banner'), J(rn.phases.filter((p) => p.t >= tNone)));

  // A label missing from the previous payload never banners: a relabeled row and a brand-new row stay quiet.
  const tQ = await fire(page, 'sessions', summary([row('paxiv · ab12', 'done'), row('neo', 'needs_permission', 'Bash'), row('rails', 'working')]));
  await sleep(600);
  const q = await rec(page);
  check('S6 a relabeled row ("paxiv" to "paxiv · ab12", done) and a new label (needs_permission) show no banner', !q.phases.some((x) => x.t >= tQ && x.phase === 'banner') && !surfaceCalls(q, tQ).some(notPill), J({ phases: q.phases.filter((x) => x.t >= tQ), surfaces: surfaceCalls(q, tQ) }));

  // A known label changing to a banner status does; the first such row in payload order wins.
  await fire(page, 'sessions', summary([row('paxiv · ab12', 'needs_input'), row('neo', 'needs_permission', 'Bash'), row('rails', 'done')]));
  await sleep(600);
  const p = await snap(page);
  check('S6a a known label changing to a banner status banners; the first changed row in payload order wins', p.phase === 'banner' && p.el['#bTitle'].text === 'paxiv · ab12' && p.el['#bSub'].text === 'needs your input' && p.el['#bIcon'].colors.includes(AMBER), J({ phase: p.phase, title: p.el['#bTitle'] && p.el['#bTitle'].text, sub: p.el['#bSub'] && p.el['#bSub'].text }));
  await waitPhase(page, 'collapsed', 3600);
  await sleep(250);

  await fire(page, 'sessions', summary([row('paxiv · ab12', 'needs_input'), row('neo', 'needs_permission', 'Bash'), row('rails', 'error')]));
  await sleep(600);
  const e = await snap(page);
  check('S7 working or done to error: banner "error" with an amber dot', e.phase === 'banner' && e.el['#bTitle'].text === 'rails' && e.el['#bSub'].text === 'error' && e.el['#bIcon'].colors.includes(AMBER), J({ phase: e.phase, title: e.el['#bTitle'] && e.el['#bTitle'].text, sub: e.el['#bSub'] && e.el['#bSub'].text }));
  await collectProps(page);
  await page.close();
}

// The right ear: art opacity, equalizer or pause glyph, the attention count.
async function scenarioEar() {
  const page = await openPage({ tag: 'ear' });
  const sq = await art(page, 'square');
  const wide = await art(page, 'wide');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(sq)));
  await sleep(800);
  const pl = await snap(page);
  check('E1 playing: ear art at opacity 1, equalizer bars shown at x 351.5, pause glyph hidden', near(pl.el['#art'] && pl.el['#art'].opacity, 1, 0.01) && visible(pl.el['#earBars']) && near(pl.el['#earBars'].x, 351.5, 1.5) && hidden(pl.el['#earPause']), J({ art: explain(pl.el['#art']), bars: explain(pl.el['#earBars']), pause: explain(pl.el['#earPause']) }));
  await collectProps(page);
  await fire(page, 'media', media(track(sq, { playing: false })));
  await sleep(600);
  const pa = await snap(page);
  check('E2 paused: ear art at opacity 0.6, pause glyph shown at x 354, bars hidden', near(pa.el['#art'] && pa.el['#art'].opacity, 0.6, 0.01) && visible(pa.el['#earPause']) && near(pa.el['#earPause'].x, 354, 1.5) && hidden(pa.el['#earBars']) && pa.phase === 'collapsed', J({ phase: pa.phase, art: explain(pa.el['#art']), pause: explain(pa.el['#earPause']), bars: explain(pa.el['#earBars']) }));
  await fire(page, 'media', media(track(wide, { playing: false })));
  await sleep(700);
  const wp = await snap(page);
  check('E3 wide art, paused: 26x15 at (324.5, 8.5), pause glyph at x 358', boxIs(wp.el['#art'], 324.5, 8.5, 26, 15, 0.9) && near(wp.el['#earPause'] && wp.el['#earPause'].x, 358, 1.5), J({ art: explain(wp.el['#art']), pause: explain(wp.el['#earPause']) }));
  await fire(page, 'media', media(track(wide, { playing: true })));
  await sleep(600);
  const wl = await snap(page);
  check('E4 wide art, playing: opacity 1, bars at x 355.5', near(wl.el['#art'] && wl.el['#art'].opacity, 1, 0.01) && near(wl.el['#earBars'] && wl.el['#earBars'].x, 355.5, 1.5), J({ art: explain(wl.el['#art']), bars: explain(wl.el['#earBars']) }));
  const r = await rec(page);
  check('E5 media updates of the same track show no banner', !r.phases.some((p) => p.phase === 'banner'), J(r.phases));
  await page.close();

  const p2 = await openPage({ tag: 'attention' });
  const sq2 = await art(p2, 'square');
  await fire(p2, 'sessions', summary([row('paxiv', 'needs_input'), row('rails', 'needs_permission', 'Bash')]));
  await fire(p2, 'media', media(track(sq2)));
  await sleep(800);
  const at = await snap(p2);
  check('A1 attention > 0: #warn reads "⚠ 2", ear art and glyphs hidden', visible(at.el['#warn']) && at.el['#warn'].text === '⚠ 2' && hidden(at.el['#art']) && hidden(at.el['#earBars']) && hidden(at.el['#earPause']), J({ warn: explain(at.el['#warn']), text: at.el['#warn'] && at.el['#warn'].text, art: explain(at.el['#art']), bars: explain(at.el['#earBars']), pause: explain(at.el['#earPause']) }));
  check('A2 collapsed: #earL and #warn at opacity 0.82', near(at.el['#earL'] && at.el['#earL'].opacity, 0.82, 0.01) && near(at.el['#warn'] && at.el['#warn'].opacity, 0.82, 0.01), J({ earL: explain(at.el['#earL']), warn: explain(at.el['#warn']) }));
  await fire(p2, 'hover', true);
  await sleep(900);
  const ex = await snap(p2);
  check('A3 expanded: #earL and #warn at opacity 1', near(ex.el['#earL'] && ex.el['#earL'].opacity, 1, 0.01) && near(ex.el['#warn'] && ex.el['#warn'].opacity, 1, 0.01), J({ earL: explain(ex.el['#earL']), warn: explain(ex.el['#warn']) }));
  await fire(p2, 'hover', false);
  await sleep(1100);
  const bk = await snap(p2);
  check('A4 collapsed again: ears back at 0.82, ear art still hidden by attention', near(bk.el['#earL'] && bk.el['#earL'].opacity, 0.82, 0.01) && near(bk.el['#warn'] && bk.el['#warn'].opacity, 0.82, 0.01) && hidden(bk.el['#art']), J({ earL: explain(bk.el['#earL']), art: explain(bk.el['#art']) }));
  await p2.close();
}

// A layout change while expanded.
async function scenarioRelayout() {
  const page = await openPage({ tag: 'relayout' });
  const sq = await art(page, 'square');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(sq)));
  await sleep(500);
  await fire(page, 'hover', true);
  await sleep(900);
  const taller = { ...LAYOUT, open: { width: 420, height: 248 } };
  const t = await page.evaluate(([l]) => { window.__setWindow(452, 264); return window.__fire('layout', l); }, [taller]);
  await sleep(900);
  const r = await rec(page);
  const s = await snap(page);
  const clip = trans(r, '#panel', 'clip-path', t)[0];
  const c = parseClip(s.el['#panel'] && s.el['#panel'].clip);
  check('N1 layout change while expanded: the clip springs to the new open height', !!clip && !isSpring(clip) && !!c && near(c.w, 420, 0.6) && near(c.h, 248, 0.6) && s.phase === 'expanded', J({ spring: clip && isSpring(clip), clip: s.el['#panel'] && s.el['#panel'].clip, phase: s.phase }));
  await fire(page, 'hover', false);
  await sleep(300);
  await page.close();
}

// Reduced motion.
async function scenarioReduced() {
  const page = await openPage({ tag: 'reduced', reduced: true });
  const sq = await art(page, 'square');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(sq)));
  await sleep(600);
  const t0 = await fire(page, 'hover', true);
  await sleep(800);
  const r = await rec(page);
  const s = await snap(page);
  const clips = trans(r, '#panel', 'clip-path', t0);
  check('R1 reduced motion: no clip-path transition on expand, the clip is the open shape at once', clips.length === 0 && s.phase === 'expanded' && !clipIs(s.el['#panel'] && s.el['#panel'].clip, 'open'), J({ clips, phase: s.phase, clip: s.el['#panel'] && s.el['#panel'].clip }));
  const op = trans(r, '#content', 'opacity', t0)[0];
  check('R2 reduced motion: #content opacity transition is 120 ms linear, no delay', timingIs(op, 120, 0, 'linear'), J({ op }));
  check('R3 reduced motion: #content has no transform transition and no offset', trans(r, '#content', 'transform', t0).length === 0 && (!s.el['#content'] || s.el['#content'].transform === 'none' || s.el['#content'].transform === 'matrix(1, 0, 0, 1, 0, 0)'), J({ tf: trans(r, '#content', 'transform', t0), now: s.el['#content'] && s.el['#content'].transform }));
  const t1 = await fire(page, 'hover', false);
  await sleep(1000);
  const r2 = await rec(page);
  const col = phaseAt(r2, 'collapsed', t1 + 200);
  const pillCall = surfaceCalls(r2, t1).find((x) => x.args && x.args.window === 'pill');
  check('R4 reduced motion: collapse waits 250 + 120 + 120 ms, then the window shrinks (no transition to wait for)', !!col && near(col.t - t1, 490, 60) && !!pillCall && pillCall.t >= col.t - 40 && trans(r2, '#panel', 'clip-path', t1).length === 0, J({ col: col && col.t - t1, pill: pillCall && pillCall.t - t1, clips: trans(r2, '#panel', 'clip-path', t1) }));
  await page.close();
}

// ---- section 8, "Window": a start waits for the grown window ---------------------------------------------------------
const clipFrom = (r, from) => trans(r, '#panel', 'clip-path', from)[0];
const gap = (a, b) => (a == null || b == null ? null : Math.round(a - b));

async function scenarioGrowWait() {
  const page = await openPage({ tag: 'grow-wait' });
  const sq = await art(page, 'square');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(sq)));
  await sleep(700);
  await page.evaluate(() => { window.__surfaceDelay = 120; });

  // Peek: the surface call at 80 ms resolves 120 ms later; the clip starts only then.
  const t0 = await fire(page, 'hover', true);
  await sleep(900);
  const r = await rec(page);
  const call = surfaceCalls(r, t0).find((x) => surfaceIs(x, 'open', 'peek'));
  const clip = clipFrom(r, t0);
  check('G1 peek: no clip-path transition starts before the open/peek surface call resolved, and one starts within ~2 frames after',
    !!call && call.resolved != null && !!clip && clip.run >= call.resolved && clip.run - call.resolved <= 50, J({ resolved: call && gap(call.resolved, t0), clipRun: clip && gap(clip.run, t0) }));
  check('G2 peek: the viewport is already the grown size 452x242 when the clip starts', !!clip && clip.vw === 452 && clip.vh === 242 && !clipIs(clip.to, 'peek'), J({ vw: clip && clip.vw, vh: clip && clip.vh, to: clip && clip.to }));
  const exp = phaseAt(r, 'expanded', t0, t0 + 900);
  check('G3 peek: the page still expands (hover held) and the window ends open', !!exp && (await snap(page, [])).vw === 452, J({ exp: exp && gap(exp.t, t0) }));
  await fire(page, 'hover', false);
  await waitPhase(page, 'collapsed', 2500);
  await sleep(700);
  const back = await snap(page, []);
  check('G4 back to the pill window after the delayed shrink', back.vw === 305 && back.vh === 32, J(back));

  // Banner: the 2600 ms close counts from the start, not from the call.
  const tB = await fire(page, 'media', media(track(sq, { title: 'Second Song' })));
  await sleep(800);
  const rb = await rec(page);
  const bcall = surfaceCalls(rb, tB).find((x) => surfaceIs(x, 'banner', 'banner'));
  const bclip = clipFrom(rb, tB);
  check('G5 banner: no clip-path transition starts before the banner/banner surface call resolved, and one starts within ~2 frames after',
    !!bcall && bcall.resolved != null && !!bclip && bclip.run >= bcall.resolved && bclip.run - bcall.resolved <= 50, J({ resolved: bcall && gap(bcall.resolved, tB), clipRun: bclip && gap(bclip.run, tB) }));
  check('G6 banner: the viewport is already the banner window 412x94 when the clip starts', !!bclip && bclip.vw === 412 && bclip.vh === 94 && !clipIs(bclip.to, 'banner'), J({ vw: bclip && bclip.vw, vh: bclip && bclip.vh, to: bclip && bclip.to }));
  await waitPhase(page, 'collapsing', 3600);
  const rc = await rec(page);
  const col = phaseAt(rc, 'collapsing', tB + 1000);
  check('G7 banner: the 2600 ms close counts from the start (after the surface resolved)', !!bcall && !!col && near(col.t - bcall.resolved, 2600, 90), J({ col: col && gap(col.t, tB), resolved: bcall && gap(bcall.resolved, tB) }));
  await waitPhase(page, 'collapsed', 2500);
  await sleep(700);

  // Hover in during a delayed banner's wait: expand ~80 ms after the hover, not the 2600 ms close.
  const tC = await fire(page, 'media', media(track(sq, { title: 'Third Song' })));
  await sleep(30);
  const tH = await fire(page, 'hover', true);
  await sleep(1500);
  const rh = await rec(page);
  const ex = phaseAt(rh, 'expanded', tH, tH + 1400);
  check('G8 hover in during a banner\'s wait: expanded about 80 ms after the hover, no collapsing before', !!ex && near(ex.t - tH, 80, 45) && !rh.phases.some((x) => x.t > tC && x.t < ex.t && x.phase === 'collapsing'), J({ phases: rh.phases.filter((x) => x.t >= tC).map((x) => [x.phase, gap(x.t, tH)]) }));
  await fire(page, 'hover', false);
  await waitPhase(page, 'collapsed', 2500);
  await sleep(300);
  await page.close();
}

// A phase change during the wait drops the start.
async function scenarioDroppedStart() {
  const page = await openPage({ tag: 'dropped-start' });
  const sq = await art(page, 'square');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(sq)));
  await sleep(700);
  await page.evaluate(() => { window.__surfaceDelay = 500; });
  const t0 = await fire(page, 'hover', true);
  await sleep(100);
  await fire(page, 'hover', false);
  await sleep(1800);
  const r = await rec(page);
  const s = await snap(page, []);
  check('D1 hover in, then out before the delayed peek surface resolved: no clip-path transition ever runs', trans(r, '#panel', 'clip-path', t0).length === 0, J(trans(r, '#panel', 'clip-path', t0)));
  check('D2 ... and the page is collapsed with the window back to the pill', s.phase === 'collapsed' && s.vw === 305 && s.vh === 32 && !r.phases.some((x) => x.t >= t0 && x.phase === 'expanded'), J({ phase: s.phase, vw: s.vw, vh: s.vh, phases: r.phases.filter((x) => x.t >= t0).map((x) => [x.phase, gap(x.t, t0)]) }));
  await page.close();
}

// A rejected `surface` is sent again on the next phase change.
async function scenarioRejected() {
  const page = await openPage({ tag: 'rejected' });
  const sq = await art(page, 'square');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(sq)));
  await sleep(600);
  await fire(page, 'hover', true);
  await sleep(900);
  await page.evaluate(() => { window.__reject = ['pill pill']; });
  await fire(page, 'hover', false);
  await waitPhase(page, 'collapsed', 2500);
  await sleep(500);
  const r0 = await rec(page);
  const rej = r0.surfaces.filter((x) => x.rejected);
  check('J1 the shrink to pill/pill was rejected by the mock', rej.length === 1 && surfaceIs(rej[0], 'pill', 'pill'), J(r0.surfaces));
  const t = await fire(page, 'hover', true);
  await sleep(60);
  const r = await rec(page);
  const again = surfaceCalls(r, t).find((x) => surfaceIs(x, 'pill', 'pill'));
  check('J2 the next phase change (intent) sends the then-current pair pill/pill again', !!again && !!phaseAt(r, 'intent', t, t + 30) && again.t - t < 40, J({ surfaces: surfaceCalls(r, t), phases: r.phases.filter((x) => x.t >= t) }));
  await sleep(900);
  await fire(page, 'hover', false);
  await sleep(300);
  await page.close();
}

// ---- section 8, "Startup": a reply older than an event is ignored ---------------------------------------------------
async function scenarioStartup() {
  const old = media(track(null, { title: 'Old Song', artist: 'Old Artist' }));
  const fresh = media(track(null, { title: 'New Song', artist: 'New Artist' }));
  const page = await openPage({ tag: 'stale-media', initial: { media: old, sessions: null, delay: 400 } });
  await sleep(100);
  await fire(page, 'media', fresh);
  await sleep(900);
  const r = await rec(page);
  const s = await snap(page, ['#content']);
  const text = (s.el['#content'] && s.el['#content'].text) || '';
  check('T1 a media event before the invoke reply: the late reply is ignored, no song banner, the card shows the event\'s track', !r.phases.some((x) => x.phase === 'banner') && text.includes('New Song') && !text.includes('Old Song'), J({ phases: r.phases.map((x) => x.phase), text }));
  await page.close();

  const p2 = await openPage({ tag: 'stale-sessions', initial: { media: null, sessions: summary([row('paxiv', 'done')]), delay: 400 } });
  await sleep(100);
  await fire(p2, 'sessions', summary([row('paxiv', 'working')]));
  await sleep(900);
  const r2 = await rec(p2);
  const s2 = await snap(p2, ['#content']);
  check('T2 a sessions event before the invoke reply: the late reply is ignored, no session banner', !r2.phases.some((x) => x.phase === 'banner') && /paxiv/.test((s2.el['#content'] && s2.el['#content'].text) || '') && !r2.surfaces.some(notPill), J({ phases: r2.phases.map((x) => x.phase), surfaces: r2.surfaces, text: s2.el['#content'] && s2.el['#content'].text }));
  await p2.close();

  // Control: with no event first, the replies are the first payloads and are used.
  const p3 = await openPage({ tag: 'reply-used', initial: { media: old, sessions: summary([row('paxiv', 'working')]), delay: 100 } });
  await sleep(500);
  const s3 = await snap(p3, ['#content']);
  const t3 = (s3.el['#content'] && s3.el['#content'].text) || '';
  check('T3 with no event first, the startup replies are used', /Old Song/.test(t3) && /paxiv/.test(t3), J(t3));
  await p3.close();
}

// ---- Artwork URLs with quotes and parentheses --------------------------------------------------------------------------
async function scenarioArtUrl() {
  const page = await openPage({ tag: 'art-url' });
  const url = 'data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><!-- ) ( \\ --><rect width="8" height="8" fill="red"/></svg>';
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(url)));
  await sleep(700);
  const bg = await page.evaluate(() => [...document.querySelectorAll('#art > *')].map((n) => getComputedStyle(n).backgroundImage));
  check('U1 an artwork URL with quotes and parentheses still sets a background image on a layer of #art', bg.length > 0 && bg.some((b) => b !== 'none' && b !== ''), J(bg));
  const s = await snap(page, ['#art']);
  check('U2 ... and the ear art is shown', near(s.el['#art'] && s.el['#art'].opacity, 1, 0.01) && boxIs(s.el['#art'], 326.5, 7, 18, 18), explain(s.el['#art']));
  await fire(page, 'hover', true);
  await sleep(900);
  const after = await snap(page, []);
  check('U3 the page keeps working: hover still expands', after.phase === 'expanded', J(after));
  await fire(page, 'hover', false);
  await sleep(300);
  await page.close();
}

// ---- Song banners and the first track (section 8, "Banners") ---------------------------------------------------------
const NONE = { via: 'adapter', health: 'ok', track: null };
async function bannerAfter(page, payload, ms = 450) {
  const t = await fire(page, 'media', payload);
  await sleep(ms);
  return (await rec(page)).phases.some((x) => x.t >= t && x.phase === 'banner');
}
async function scenarioFirstTrack() {
  // nothing, then A (no banner), then B (banner).
  let page = await openPage({ tag: 'first-track-1' });
  let sq = await art(page, 'square');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', NONE);
  await sleep(200);
  check('F1 nothing, then the first track A: no banner', !(await bannerAfter(page, media(track(sq, { title: 'Song A' })))), J((await rec(page)).phases));
  check('F2 ... then a different track B: banner', await bannerAfter(page, media(track(sq, { title: 'Song B' }))), J((await rec(page)).phases));
  await page.close();

  // A, nothing, A: none.
  page = await openPage({ tag: 'first-track-2' });
  sq = await art(page, 'square');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(sq, { title: 'Song A' })));
  await sleep(300);
  await fire(page, 'media', NONE);
  await sleep(300);
  check('F3 A, then nothing, then A again: no banner', !(await bannerAfter(page, media(track(sq, { title: 'Song A' })))), J((await rec(page)).phases));
  await page.close();

  // A, nothing, B: banner.
  page = await openPage({ tag: 'first-track-3' });
  sq = await art(page, 'square');
  await fire(page, 'sessions', summary([row('paxiv', 'working')]));
  await fire(page, 'media', media(track(sq, { title: 'Song A' })));
  await sleep(300);
  await fire(page, 'media', NONE);
  await sleep(300);
  check('F4 A, then nothing, then B: banner', await bannerAfter(page, media(track(sq, { title: 'Song B' }))), J((await rec(page)).phases));
  await page.close();
}

const scenarios = [scenarioLayout, scenarioHover, scenarioSongBanner, scenarioWideBanner, scenarioSessionBanner, scenarioEar, scenarioRelayout, scenarioReduced, scenarioGrowWait, scenarioDroppedStart, scenarioRejected, scenarioStartup, scenarioArtUrl, scenarioFirstTrack];
for (const s of scenarios) {
  try { await s(); } catch (e) { check(`${s.name} ran to the end`, false, e && e.stack ? e.stack.split('\n').slice(0, 3).join(' | ') : String(e)); }
}

// Transition properties across every page: only transform, opacity and clip-path may ever animate.
{
  const allowed = new Set(['transform', 'opacity', 'clip-path']);
  const extra = [...seenProps].filter((p) => !allowed.has(p));
  check('X1 only transform, opacity and clip-path are animated (transitions and keyframes)', extra.length === 0, J({ seen: [...seenProps], extra }));
}
check('X2 no page errors or console errors', pageErrors.length === 0, J(pageErrors.slice(0, 5)));

await browser.close();
for (const r of results) console.log(`${r.ok ? 'PASS' : 'FAIL'} ${r.name}${r.ok ? '' : ' :: ' + r.detail}`);
const failed = results.filter((r) => !r.ok).length;
console.log(`${results.length - failed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);

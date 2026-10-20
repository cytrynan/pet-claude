# Code Implementation — HTML/CSS/JS & React + Tailwind

Concrete tokens and recipes. Read alongside the HIG references (app UI) or marketing-pages (landing pages).

## Design tokens (drop-in CSS)

```css
:root {
  --font-stack: -apple-system, BlinkMacSystemFont, "SF Pro Display", "SF Pro Text",
                Inter, "Helvetica Neue", Arial, sans-serif;

  /* HIG semantic tokens — light */
  --label: #000;
  --label-2: rgba(60,60,67,.60);
  --label-3: rgba(60,60,67,.30);
  --separator: rgba(60,60,67,.29);
  --bg: #fff;            --bg-2: #f2f2f7;      --bg-3: #fff;
  --bg-grouped: #f2f2f7; --bg-grouped-cell: #fff;
  --tint: #0088ff;  --green: #34c759;  --red: #ff383c;  --orange: #ff8d28;

  /* Marketing palette */
  --mk-text: #1d1d1f;  --mk-muted: #86868b;  --mk-surface: #f5f5f7;
  --mk-link: #0066cc;  --mk-cta: #0071e3;  --mk-cta-hover: #0077ed;
  --mk-dark-text: #f5f5f7;  --mk-dark-muted: #a1a1a6;  --mk-dark-link: #2997ff;

  --radius-pill: 980px;
  --ease-apple: cubic-bezier(0.28, 0.11, 0.32, 1);
}
.dark, [data-theme="dark"] {
  --label: #fff; --label-2: rgba(235,235,245,.60); --label-3: rgba(235,235,245,.30);
  --separator: rgba(84,84,88,.60);
  --bg: #000; --bg-2: #1c1c1e; --bg-3: #2c2c2e;
  --bg-grouped: #000; --bg-grouped-cell: #1c1c1e;
  --tint: #0091ff; --green: #30d158; --red: #ff4245; --orange: #ff9230;
}
body { font-family: var(--font-stack); -webkit-font-smoothing: antialiased; }
/* If Inter actually renders (non-Apple device / client work): */
body { font-feature-settings: "cv05", "cv11"; }
```

## Type scale

App UI (HIG, px = pt at 1x): Large Title 34/41 · Title1 28/34 · Title2 22/28 · Title3 20/25 · Headline 17/22 semibold · Body 17/22 · Callout 16/21 · Subheadline 15/20 · Footnote 13/18 · Caption 12/16.

Marketing headlines:
```css
h1.hero    { font-size: clamp(48px, 7vw, 96px); font-weight: 700; letter-spacing: -.022em; line-height: 1.05; }
h2.section { font-size: clamp(32px, 5vw, 56px); font-weight: 600; letter-spacing: -.015em; line-height: 1.08; }
p.subhead  { font-size: 21px; line-height: 1.4; color: var(--mk-muted); letter-spacing: .011em; }
```

## Liquid Glass recipe (CSS approximation)

Floating bar / island — the signature current-Apple element:

```css
.glass {
  background: rgba(255,255,255,.55);              /* dark mode: rgba(30,30,32,.55) */
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border-radius: 28px;                             /* capsule for bars: 999px */
  box-shadow:
    0 8px 32px rgba(0,0,0,.08),                    /* soft lift */
    inset 0 1px 0 rgba(255,255,255,.55),           /* specular top edge */
    inset 0 0 0 .5px rgba(255,255,255,.25);        /* glass rim */
}
.glass--clear { background: rgba(255,255,255,.18); } /* over media; add a 35%-black
                                                        dimming layer if content is bright */
```

Floating iOS-style tab bar:
```css
.tabbar {
  position: fixed; left: 50%; bottom: max(16px, env(safe-area-inset-bottom));
  transform: translateX(-50%);
  display: flex; gap: 8px; padding: 8px 12px; border-radius: 999px;
  /* + .glass properties */
}
.tabbar a { width: 56px; height: 44px; display: grid; place-items: center;
  border-radius: 999px; color: var(--label-2); }
.tabbar a[aria-current] { color: var(--tint); }   /* icons monochrome; active = tint */
```

Scroll edge effect (content fading under a floating top bar):
```css
.scroll-edge {
  position: fixed; inset: 0 0 auto 0; height: 96px; pointer-events: none;
  backdrop-filter: blur(8px);
  mask-image: linear-gradient(to bottom, black 30%, transparent 100%);
  -webkit-mask-image: linear-gradient(to bottom, black 30%, transparent 100%);
}
```
Key behavior: content scrolls edge-to-edge *beneath* bars (no `padding-top` shoving content down; use safe-area padding inside the scroller instead).

## Buttons

```css
.btn-filled {           /* prominent: ONE per view */
  background: var(--tint); color: #fff; font-size: 17px; font-weight: 600;
  padding: 13px 24px; border-radius: 999px; min-height: 44px;
  transition: opacity .2s var(--ease-apple), transform .2s var(--ease-apple);
}
.btn-filled:active { opacity: .8; transform: scale(.97); }  /* press = dim, never ripple */
.btn-gray   { background: var(--bg-2); color: var(--tint); /* same metrics */ }
.btn-plain  { background: none; color: var(--tint); }
.btn-destructive { color: var(--red); }
```
Marketing CTA: same pill, `--mk-cta` background, 12px/24px padding, hover lightens to `--mk-cta-hover` — no lift, no shadow.

> **Contrast trap (local fix 2026-10-20):** `.btn-filled` as written (white 17px/600 on `--tint #0088ff`) is **3.52:1 → fails WCAG AA** (17px semibold is not "large text"). Pick one: (a) fill `#0071e3` (4.7:1) at 17px, or (b) keep the bright systemBlue look and set the label to **≥19px bold** (WCAG large ⇒ 3:1 suffices: `#0088ff` 3.52 / dark `#0091ff` 3.23). On light backgrounds a slightly deeper fill (`#006cdc`) reads closer to iOS than raw `#0088ff`.

## Local refinements (2026-10-20, from a Chrome-extension popup redesign — not upstream)

**Stained-glass prominent button** (iOS 26 alert primary look; works on a plain cell where SVG refraction shows nothing). Layers: translucent tint + top gloss + bottom bounce light + 1px rim that catches light at top-left/bottom-right + faint tinted glow. No text-shadow.
```css
:root { --glass-tint: rgba(0,108,220,.97); --glass-glow: rgba(0,108,220,.30); }
@media (prefers-color-scheme: dark) { :root { --glass-tint: rgba(0,145,255,.93); --glass-glow: rgba(0,145,255,.26); } }
.btn-glass { position: relative; isolation: isolate; min-height: 54px; padding: 15px 20px;
  font-size: 19px; line-height: 24px; font-weight: 700; color: #fff; border-radius: 999px;
  background: linear-gradient(180deg, rgba(255,255,255,.18) 0%, rgba(255,255,255,.05) 35%,
      rgba(255,255,255,.02) 50%, rgba(255,255,255,0) 60%, rgba(255,255,255,.07) 100%), var(--glass-tint);
  box-shadow: inset 0 1px 1px rgba(255,255,255,.42), inset 0 -2px 6px rgba(255,255,255,.10),
      0 8px 20px -12px var(--glass-glow); }
.btn-glass::before { content:''; position:absolute; inset:0; z-index:-1; padding:1px; border-radius:inherit;
  background: linear-gradient(160deg, rgba(255,255,255,.6), rgba(255,255,255,.12) 32%, rgba(255,255,255,0) 55%, rgba(255,255,255,.3));
  -webkit-mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0);
  -webkit-mask-composite: xor; mask-composite: exclude; pointer-events:none; }  /* rim */
.btn-glass::after { content:''; position:absolute; inset:0; z-index:-1; border-radius:inherit; pointer-events:none;
  background: radial-gradient(55% 90% at 18% 0%, rgba(255,255,255,.10), transparent 55%); }  /* reflection */
.btn-glass:active { filter: brightness(1.08); transform: scale(.985); }  /* "illuminates from within" */
```
Strength knob: doubling every alpha above (tint .90, gloss .30, rim .85, reflection .20, glow .55) gives a "more transparent" look; the values above are the toned-down version a reviewer preferred. Hover: strengthen the rim, never mix white into the fill (pushes white text under 3:1).

**Gray glass secondary button** (iOS 26 alert *Cancel*): measured on a real screenshot it is just **one step brighter than the panel behind it** (≈ +10% white), picks up the background hue, faint rim, bold label.
```css
.btn-glass-gray { /* same metrics, ::before rim with rgba(255,255,255,.95)→.08→0→50% */
  background: linear-gradient(180deg, rgba(255,255,255,.14), rgba(255,255,255,.03) 40%, transparent 60%, rgba(255,255,255,.05)),
              rgba(120,120,128,.10);                        /* dark: rgba(255,255,255,.10) */
  backdrop-filter: blur(18px) saturate(190%);
  box-shadow: inset 0 1px 1px rgba(255,255,255,.75), inset 0 -1px 2px rgba(255,255,255,.35),
              inset 0 0 0 .5px rgba(0,0,0,.05), 0 6px 16px -12px rgba(0,0,0,.22); }
```
Tint-colored label on it in light mode: `#0066cc` lands at exactly 4.50:1 on gray-10% over `#f2f2f7` → use `#005fc0` (≈4.9:1). Gray 16% drops `#0066cc` to 4.13:1.

**Floating bottom action bar inside a padded scroller** (popup/sheet): drop the full-width glass strip; let the buttons float and use ONE soft scroll-edge layer behind them.
- `position: sticky; bottom: 0` stops at the scroller's **padding-bottom** above the edge (measured: bar bottom 564px vs scroller 580px). A transparent bar leaks content through that gap → extend the edge layer: `::before { top: -<gap between content and bar>; bottom: -<scroller padding-bottom>; z-index: -1; }`.
- Start the layer *inside* the gap (not 28px above) or it darkens unscrolled content (very visible in dark mode).
- Make it dense where the buttons sit: `background: linear-gradient(to top, color-mix(in srgb, var(--bg-grouped) 94%, transparent) 75%, transparent); backdrop-filter: blur(10px); mask-image: linear-gradient(to top, #000 78%, transparent);` — at 55%/60% text stayed readable between the buttons.

**Choice sets with one prominent option** (e.g. Again / Good / Easy): equal size, equal label size (19px bold for the whole set so the tinted one qualifies as large text), only the recommended option gets the tinted glass, the others gray glass with `--label` text — no tinted *text* on the gray ones. Put per-option secondary info (e.g. "next in 15 days") as a 13px `--label-2` caption row **above** the buttons in the same 3-column grid, not inside them: small white text on a systemBlue glass fill never reaches 4.5:1 (measured 4.2–4.6:1 at best).

**List rows with actions (management pages):** no per-row Edit/Delete pills (a red "Delete" repeated on every row is noise). Row click = edit; trailing "⋯" = 36px circle, `--label-2` glyph, `--fill` only on hover/open → glass popover menu (min-width 200px, radius 18, padding 6, items 40px with 18px line icons, only the destructive item in `--red-text`). The list needs `overflow: visible` so the menu can escape — round the first/last row instead of clipping the list. Compact rows to title / 2-line clamped detail / one 13px meta line (chips · source link · status).

**Page whose title names the main content but opens on settings:** put a segmented control ("Content | Settings") inside the floating glass header and open on the content. Segmented: track `--fill`, 2px padding, capsule; segments 30px high, 13px/600; selected = `--bg-cell` with `0 1px 3px rgba(0,0,0,.12)` in light; **in dark use `#636366` for the selected segment** — `--bg-cell` (#1c1c1e) on a dark cell or dark glass header is nearly invisible (found after approval, 2026-10-20). Also use it for 2–3-way exclusive choices instead of a `<select>` (e.g. CSV comma | tab).

**Settings rows (web):** compact time value = one `--fill` capsule (radius 8, 15px tnum) wrapping the hour/minute `<select>`s with their chevrons removed — not two loose dropdowns. A row that opens an outside settings page (e.g. `chrome://extensions/shortcuts`) = a tint-text row inside the group with a trailing ↗, never a bare link under the section with the URL in the label. Japanese footers: `text-wrap: pretty` stops a lone "ん。" from dropping to its own line.

**Determinate progress (review/quiz flows):** 4px capsule track `--fill-2`, fill `--tint`, label "N left" 13px `--label-2` tnum on the trailing side; count re-queued items as remaining so the bar never moves backwards (HIG: even out the pace).

**Verify contrast on rendered pixels, not on paper:** gloss/reflection layers change the color under the glyphs. A computed 3.13:1 measured 2.86:1 on the screenshot. Sample the pixels in the text band (35–65% of button height, skipping glyph pixels) in both schemes. Before shooting sheets/dialogs, disable their entrance animation (a 0.28s opacity fade got captured half-transparent in one scheme) and make the viewport at least as tall as the clip, or `position: fixed` dimming layers stop short of the bottom.

## Grouped list (Settings style)

```css
.group { background: var(--bg-grouped-cell); border-radius: 10px; margin: 0 16px; overflow: hidden; }
.row { display: flex; align-items: center; gap: 12px; min-height: 44px; padding: 11px 16px; font-size: 17px; }
.row + .row { border-top: .5px solid var(--separator); margin-left: 16px; padding-left: 0; }
.row .icon { width: 29px; height: 29px; border-radius: 7px; display: grid; place-items: center; color: #fff; }
.row .value { margin-left: auto; color: var(--label-2); }
.row .chevron { color: var(--label-3); }
.row:active { background: rgba(0,0,0,.05); }
```
Page background: `var(--bg-grouped)`. Toggle: 51×31px pill, `--green` on-state, white knob with `0 3px 8px rgba(0,0,0,.15)` shadow.

## Motion

Ease/duration defaults: `--ease-apple`, 0.3–0.4s; springs for interactive elements (Framer Motion: `type:"spring", stiffness:340, damping:30`).

GSAP reveal (subtle rise, settles fast, no bounce):
```js
gsap.utils.toArray('[data-reveal]').forEach(el => {
  gsap.from(el, { y: 28, opacity: 0, duration: .7, ease: 'power2.out',
    scrollTrigger: { trigger: el, start: 'top 85%', once: true } });
});
```
Pinned scrub and canvas image-sequence patterns: pin the section, `scrub: 0.6`, map progress → transform or frame index (≤120 frames). Pair with Lenis on marketing pages. Direction rule: whatever slides in from an edge dismisses back to that edge.

Always gate:
```js
if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
  ScrollTrigger.getAll().forEach(t => t.kill());
  document.querySelectorAll('[data-reveal]').forEach(el => el.removeAttribute('style'));
}
```

## React + Tailwind mapping

```js
// tailwind theme extension
extend: {
  colors: {
    label: 'var(--label)', label2: 'var(--label-2)',
    bg: { DEFAULT: 'var(--bg)', 2: 'var(--bg-2)', grouped: 'var(--bg-grouped)', cell: 'var(--bg-grouped-cell)' },
    tint: 'var(--tint)', sysgreen: 'var(--green)', sysred: 'var(--red)',
  },
  borderRadius: { cell: '10px', card: '22px', pill: '980px' },
  transitionTimingFunction: { apple: 'cubic-bezier(0.28,0.11,0.32,1)' },
}
```
Conventions: `<Button variant="filled|gray|plain|destructive">`; `<GlassBar>` wrapper applying the glass recipe; icons via lucide-react at consistent strokeWidth (1.8–2), sized to the adjacent text. Section wrapper for marketing: `py-24 md:py-36 text-center` with `max-w-[980px] mx-auto px-6`.

## Sheet (web approximation)

Bottom sheet: full-width card, top radius 16–20px, grabber (36×5px pill, `--separator`), scrim `rgba(0,0,0,.4)`, slides up 0.35s `--ease-apple`, drag-to-dismiss mirrors downward. Cancel leading / Done trailing in its top bar.

## Self-check for code output

Bars float (fixed, inset, rounded, glass) with content scrolling beneath — not opaque strips. One tinted primary per view. Press states dim/scale. 44px targets. `env(safe-area-inset-*)` respected. Reduced motion handled. Dark mode via tokens, not per-element overrides.

//! App design tokens, ported from `assets/css/tailwind.css`. Injected once at
//! the app root, layered after `panel_kit::CSS`. The `.light-mode` class on
//! `<html>` flips the theme (toggled by the header, persisted to localStorage).
//! Not `.light` — panel-kit owns that for its traffic-light dots; see
//! `main::set_light_class`.
//!
//! The second block remaps panel-kit's own variables onto these tokens so the
//! panel chrome on the play screen matches the rest of the app.

// BOOT CSS: the pre-wasm boot card in client/flake.nix (the shell document, marked
// "BOOT CSS" there) hardcodes copies of the six tokens below — --bg-app, --bg-card,
// --text-primary, --text-secondary, --border-app, --pastel-red — because
// panel_kit::CSS and this stylesheet do not exist until the app mounts. That
// duplication is deliberate; the two files point at each other. If the palette
// changes here, change it there too, or grep `BOOT CSS`.

pub const DESIGN: &str = r#"
@import url('https://fonts.googleapis.com/css2?family=Inconsolata:wght@400;700&family=Bricolage+Grotesque:opsz,wdth,wght@12..96,75..100,200..800&display=swap');

:root {
  --bg-app: #121212;
  --bg-card: #18181b;
  --bg-cell-empty: #09090b;
  --bg-cell-letter: #202024;
  --text-primary: #f4f4f5;
  --text-secondary: #a1a1aa;
  --border-app: #27272a;
  --border-hover: #3f3f46;
  --pastel-red: #ff8c8c;
  --pastel-green: #a8e6cf;
  --pastel-yellow: #feea99;
  --color-primary: var(--pastel-yellow);
  --color-success: var(--pastel-green);
  --color-warning: var(--pastel-yellow);
  --color-error: var(--pastel-red);

  /* Palette tokens: dark ink for text on pastel fills, plus podium metals. */
  --contrast-ink: #0f172a;
  /* Selection fills: the solid backgrounds that mark a *state* — the focused
     crossword cell, the active Clues direction tab. Their meaning is read off
     relative brightness inside a set of siblings (the other cells, the other
     tab), so these have to stay PALE with DARK ink in BOTH themes; --pastel-*
     + --contrast-ink cannot, because light mode darkens the pastels and flips
     the ink to white, which puts the darkest thing on the page exactly where
     the lightest one belongs. Literal hex on purpose, not `var(--pastel-*)` /
     `var(--contrast-ink)` aliases: custom properties resolve at use time, so an
     alias would inherit the .light-mode flip and reintroduce the inversion.
     Dark keeps today's values; the ink is dark in both themes so it is only
     declared here. Borders stay --pastel-*, which is what outlines these
     fills once it darkens in light mode. */
  --fill-yellow: #feea99;
  --fill-green: #a8e6cf;
  --fill-ink: #0f172a;
  --podium-silver: #cbd5e1;
  --podium-bronze: #d97706;
  /* Modal/overlay scrim — deliberately theme-fixed: a dark veil reads
     correctly over both themes. */
  --scrim: rgba(0, 0, 0, .5);

  /* Elevation. panel-kit hardcodes black shadows on `.panel` (#0007) and
     `.tip-overlay` (#000c) that no variable reaches; the two rules near the
     bottom of this sheet re-declare them against these vars so the shadow can
     be retuned per theme instead of being tuned once for a near-black page.
     Dark keeps panel-kit's original look: a wide, heavy diffusion. */
  --shadow-panel: 0 6px 24px rgba(0, 0, 0, .45);
  --shadow-pop: 0 10px 30px rgba(0, 0, 0, .75);

  /* Co-op presence rings on the board (game_play.rs REMOTE_COLORS). Hue-
     distinct from each other and from --pastel-yellow, which the local
     player owns. */
  --presence-1: #a8e6cf;
  --presence-2: #a8c8f0;
  --presence-3: #d0b8f0;
  --presence-4: #f0b8d0;

  /* App fonts. --mono is defined in the panel-kit remap block below. */
  --font-sans: 'Bricolage Grotesque', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;

  /* Type scale (rem) + weights are numeric. The values are RENDERED px at the
     13px root panel-kit sets on <html> (see the 44px note on .app-btn below:
     every rem in the app resolves against 13px): .85rem=11, .95rem=12.4,
     1rem=13, 1.08rem=14, 1.25rem=16.25, 1.5rem=19.5, 2rem=26.
     THE FLOOR IS LOAD-BEARING. The old ladder rendered its three smallest
     roles at 8.125/9.75/10.4px — measured in Chromium via getComputedStyle,
     and the impeccable detector flags functional text below 11px and body
     text below 12px, so every one of those sites was a WCAG-adjacent
     legibility failure before this change. Keep new work on these vars; do
     not re-introduce sub-12px literals. */
  --fs-2xs: .85rem;
  --fs-xs: .95rem;
  --fs-sm: 1rem;
  --fs-md: 1.08rem;
  --fs-lg: 1.25rem;
  --fs-xl: 1.5rem;
  --fs-2xl: 2rem;
}
/* Light palette. Two things it has to get right that a naive lightening misses.

   ELEVATION DIRECTION. In dark, the card sits *above* the page (#18181b on
   #121212). The old light values had it backwards — a #f4f4f5 card on a pure
   #ffffff page — so panels read as dents, and the drop shadow had nothing but
   pure white to fall on, which is exactly what makes it look like a grey
   smudge. Here the page is the grey one and the card is the bright one, so
   elevation runs the same way in both themes.

   PASTEL DUTY. The --pastel-* tokens do double duty: as fills (with
   --contrast-ink on top) and as raw text/border colours. Tuned against black,
   they land at 1.2–2.2:1 on white — the yellow brand mark, every accent label,
   and panel-kit's whole --accent chain (.skeleton, .spin-label, .ide-lang,
   .snap-toggle.on, .panel-loading-fill) were effectively invisible. So light
   mode darkens the pastels and flips --contrast-ink to white. That keeps both
   duties legible from one place, without touching ~40 call sites. */
.light-mode {
  /* Four surfaces, ordered the same way dark orders them: recessed cell <
     page < card < raised cell. The card is deliberately held off pure white
     so --bg-cell-letter has somewhere brighter to go — it is the `:hover` fill
     for .game-row and the games/home lists, and a hover that brightens matches
     dark (#202024 over #18181b). */
  --bg-app: #ededf0;         /* page/workspace — the grey one */
  --bg-card: #f7f7f8;        /* panel + card surface, raised above the page */
  --bg-cell-empty: #dcdce0;  /* recessed: input fills, blocked cells, tracks */
  --bg-cell-letter: #ffffff; /* raised: filled crossword cell, row hover, badge */
  --text-primary: #18181b;   /* 16.6:1 on --bg-card */
  /* was #71717a — 4.40:1 on a card, i.e. below AA for body text, the .app-btn
     label and every panel-kit --dim consumer. #52525b restores the ~7.2:1 that
     dark mode's secondary text already had. */
  --text-secondary: #52525b;
  /* was #e4e4e7 / #d4d4d8 — 1.27:1 and 1.48:1, near enough to invisible. These
     draw every card, button, input, tab and panel edge in a UI made entirely of
     boxes, so they carry the component boundary and need WCAG's 3:1 for one.
     (--border-hover also paints panel-kit's .dock-empty text and .resize grip.) */
  --border-app: #8a8a93;
  --border-hover: #5c5c66;
  /* Darkened per the note above: ≥4.8:1 as text on --bg-cell-empty (the least
     forgiving surface they land on) and ≥6.1:1 on the card. Hue is preserved,
     but a yellow that clears 4.5:1 on white is necessarily a dark amber. */
  --pastel-red: #b02a20;
  --pastel-green: #0d6a4e;
  --pastel-yellow: #775600;
  /* Ink on a pastel fill inverts along with the pastels — white, ≥5.1:1 against
     every fill above and against both podium metals. Every call site pairs it
     with a solid --pastel or --color fill, never a color-mix tint, so the flip
     is safe. */
  --contrast-ink: #ffffff;
  /* The selection fills do NOT follow the pastels down — see :root for why.
     Deepened a shade from dark's values so they still register against a
     near-white board: #ffe066 is 13.7:1 under --fill-ink, sits above the
     .cw-selected tint (L .755 vs .703) and the recessed --bg-cell-empty
     (.718), and separates from a filled white cell by chroma (ΔE76 63) since
     nothing can be brighter than #ffffff. #8fdcbf is 11.2:1 under the ink. */
  --fill-yellow: #ffe066;
  --fill-green: #8fdcbf;
  /* Podium metals matched to the same bar: silver was 1.48:1 on white, and
     bronze takes --contrast-ink like the gold/silver places beside it
     (components/ui.rs, pages/stats.rs), so the fill has to be dark enough to
     hold white text here. */
  --podium-silver: #5b6a80;
  --podium-bronze: #96560a;
  /* Shadows retuned for a light field. Dark's wide 24px/45%-black diffusion
     turns into a dirty grey haze on a pale surface, so: alpha drops by ~6x,
     blur tightens, the offset stays small and downward, and the tint is
     --text-primary's cool near-black rather than pure black. Two layers — a
     1px contact edge plus a soft ambient — read as a lifted sheet of paper
     instead of a blur. */
  --shadow-panel: 0 1px 1px rgba(24, 24, 27, .06), 0 3px 10px rgba(24, 24, 27, .07);
  --shadow-pop: 0 1px 2px rgba(24, 24, 27, .08), 0 6px 18px rgba(24, 24, 27, .12);
  /* Presence rings darkened on the same bar as the pastels: ≥4.4:1 against
     --bg-cell-empty (a blocked/unfilled cell, the least forgiving surface a
     ring lands on) and ≥6.1:1 against a filled white one. Hues held apart so
     four collaborators stay tellable. */
  --presence-1: #0d6a4e;
  --presence-2: #1a5fa8;
  --presence-3: #6b3fa0;
  --presence-4: #a8386b;
}

/* Map ALL of panel-kit's theme variables onto the app tokens so the panel
   chrome (surface, title bars, borders, badges, inverse chips) flips with the
   theme too. Anything left unmapped keeps panel-kit's dark default and breaks
   light mode. The fixed accent lights (--blue/--yellow/--pink/--red/--green)
   are intentionally left as panel-kit's — they read on both themes. */
:root {
  --bg: var(--bg-app);          /* workspace background, behind panels */
  --panel: var(--bg-card);      /* panel surface + title bar */
  --fg: var(--text-primary);
  --dim: var(--text-secondary);
  --line: var(--border-app);
  --line2: var(--border-hover);
  --accent: var(--color-primary);
  --inv-bg: var(--text-primary); /* inverse chip: contrasts the surface */
  --inv-fg: var(--bg-app);
  --badge-bg: var(--bg-cell-letter);
  --badge-fg: var(--text-primary);
  --badge-c: var(--text-secondary);
  --badge-info: var(--color-primary);
  --mono: 'Inconsolata', ui-monospace, monospace;
  /* These remain app design tokens for content sizing. Panel geometry is
     projected by the v1 core's Clamp; CSS no longer drives reducer minima. */
  --panel-min-w: 340px;
  --panel-min-h: 240px;
}

/* In tiling mode, cap panel height to the workspace so long content (e.g. the
   leaderboard) scrolls inside the panel body instead of growing the panel and
   pushing the page. Compact keeps its stacked, page-scrolling behavior. */
.ws-root:not(.compact) .ws.tiling .panel { max-height: 100%; }

/* The app deliberately layers its surface shadows after panel-kit. v1 owns
   panel geometry, traffic-light chrome, and panel-body padding; these rules
   only apply app theme shadows. The tooltip already uses `var(--panel)` in v1,
   and this later rule replaces only its literal black shadow. */
.panel { box-shadow: var(--shadow-panel); }
.tip-overlay { color: var(--fg); box-shadow: var(--shadow-pop); }

* { box-sizing: border-box; }
body {
  margin: 0;
  background-color: var(--bg-app);
  color: var(--text-primary);
  font-family: var(--font-sans);
  transition: background-color .15s ease, color .15s ease, border-color .15s ease;
}
a { color: inherit; text-decoration: none; }

::-webkit-scrollbar { width: 4px; height: 4px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--border-app); border-radius: 0; }
::-webkit-scrollbar-thumb:hover { background: var(--border-hover); }

.app-card { background-color: var(--bg-card); border: 1px solid var(--border-app); border-radius: 0; }
/* The border is the CONTROL BOUNDARY, not a card edge: WCAG 1.4.11 wants 3:1
   against the surface behind it, and --border-app is 1.19:1 on the dark card
   (3.20:1 light, which is why this read as a dark-only bug). --text-secondary is
   6.9:1 dark / 7.2:1 light — the same token the boot card's .boot-btn uses, and
   the one the light palette already picked over #71717a for the label. */
.app-btn { font-family: var(--font-sans); padding: .5rem .9rem; min-height: 44px; font-size: var(--fs-md);
  font-weight: 600; border: 1px solid var(--text-secondary); border-radius: 0; background-color: var(--bg-card);
  color: var(--text-secondary); transition: all .15s ease; cursor: pointer;
  /* min-height alone does not centre anything: the box is 44px but the label
     lays out at the top, so every pixel of slack collects BELOW the text.
     Measured on "Sign in": 8.5px above the glyphs, 21.5px below — 13px of
     dead space under the label. Centring is what makes the min-height read
     as a button rather than as padding. */
  display: inline-flex; align-items: center; justify-content: center; }
/* Hover brightens to --text-primary (15:1 dark / 16.6:1 light), not to
   --border-hover, which is 1.6:1 on the dark card: the old hover rule quietly
   dropped the boundary below 3:1 in the very state the pointer is on. */
.app-btn:hover { color: var(--text-primary); border-color: var(--text-primary); }
.app-btn:disabled { opacity: .5; cursor: not-allowed; }
/* `.app-btn:hover` is (0,2,0) and would otherwise beat this (0,1,0), so the
   accent used to vanish under the pointer — the `.section-tab-active` idiom
   below, one line up, for the same reason. */
.app-btn-active, .app-btn-active:hover { color: var(--text-primary); border-color: var(--color-primary); }
/* The house ring (panel-kit ships `.ws-root :focus-visible`, which cannot reach
   an .app-btn rendered outside a workspace — the post-mount error panel is a
   SIBLING of .ws-root under main.app-main, so it fell back to the UA ring). */
.app-btn:focus-visible { outline: 2px solid var(--text-primary); outline-offset: 2px; }
/* 44px, not 2.75rem: the boot card's .boot-btn measures 44.0px because it renders
   BEFORE panel-kit exists, when the root is still 16px. panel-kit's
   `body, html, #main` rule sets font-size:13px on <html>, so every rem in the app
   resolves against 13px and 2.75rem is 35.75px here — the delta's arithmetic
   assumed the pre-boot root. Measured in Chromium: 2.75rem -> 35.75px, 44px ->
   44.0px. This is a house-atom change: every .app-btn in the app grows, including
   the header's theme toggle / Sign in / Sign out / Resume and the confirm modal. */
/* 1.25rem = 16.25px at the 13px root — above 16px ON PURPOSE: iOS Safari
   force-zooms the viewport when a focused input's text is under 16px, which
   breaks the form layout exactly when the user tries to type. Buttons don't
   zoom, so the 44px .app-btn keeps --fs-md; only inputs pay this. */
.app-input { background-color: var(--bg-cell-empty); color: var(--text-primary); border: 1px solid var(--border-app);
  border-radius: 0; outline: none; padding: .4rem .6rem; font-size: var(--fs-lg);
  transition: border-color .15s ease; }
.app-input:focus { border-color: var(--color-primary); }
/* The UA placeholder measured 4.3:1 on --bg-cell-empty (#757575 on #09090b in
   Chromium) — below the 4.5:1 text floor. --text-secondary is the dimmest
   token that clears it in BOTH themes (6.91:1 dark / 5.65:1 on the recessed
   light fill), so the placeholder gives up the conventional "dimmer than the
   value" look: every auth field pairs it with a visible label, so the value
   needs no placeholder-based disambiguation. `opacity: 1` stops the UA
   applying its own extra fade on top of the token. */
.app-input::placeholder { color: var(--text-secondary); opacity: 1; }

/* ── Touch-target expansion (WCAG 2.5.5 / 2.5.8) ────────────────────────────
   Some controls must stay VISUALLY small — a BETA chip, a ✕, a direction
   tab — but every interactive control needs a real target. Stretching the
   box would reflow the rows they sit in, so the target is expanded with a
   transparent ::after instead: same pixel look, 44×44 (or 24×24 for
   low-stakes links) of clickable area, zero layout cost. Requires
   position:relative on the control (set here so no call site forgets). */
.tap-44, .tap-24 { position: relative; }
.tap-44::after, .tap-24::after {
  content: ""; position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%);
}
.tap-44::after { width: max(100%, 44px); height: max(100%, 44px); }
.tap-24::after { width: max(100%, 24px); height: max(100%, 24px); }

/* ── Post-mount render-error panel (PageErrorPanel, main.rs) ─────────────────
   Atoms rather than five inline `style` attributes: the panel is the one place
   that cannot be re-themed or reduced-motion-tuned without editing Rust, and an
   inline style cannot be reached by a stylesheet rule at all. Composed from the
   atoms above, so it re-themes with them. */
.app-error-panel { margin: auto; max-width: 34rem; padding: 1.5rem; display: flex;
  flex-direction: column; gap: .75rem; }
/* Nothing resets p/h1 margins in panel-kit or DESIGN (panel-kit.css zeroes only
   `body`), so the panel zeroes its own copy rows explicitly. */
.app-error-panel p { margin: 0; }
.app-error-title { margin: 0; font-size: 1.125rem; font-weight: 700; color: var(--color-error); }
.app-error-actions { display: flex; gap: .5rem; flex-wrap: wrap; }
.app-eyebrow { margin: 0; font-family: var(--mono, monospace); font-size: var(--fs-2xs);
  letter-spacing: .05em; text-transform: uppercase; }

/* panel-kit's "traffic light" window controls (`.light`) are pure-color circles
   with no text content (see panel-kit.css) — there is no font to match, so
   nothing to override here. Left as-is intentionally. */

/* ── App shell: header + per-view panel-kit workspace + footer ──────────────
   Every view is a panel-kit workspace. On DESKTOP the workspace fills the area
   between the (sticky) header and footer — panels are clamped to vw × the
   available vh, and the page itself never scrolls. On COMPACT surfaces
   (<760px) v1 projects a one-column stack in Snapshot Vec order; we clamp
   width to vw but let height scroll so stacked panels flow down the page. */
.app-shell { display: flex; flex-direction: column; height: 100vh; overflow: hidden; }
/* Column so a page that renders chrome (e.g. AdminNav) above its workspace
   stacks vertically, with the workspace taking the remaining height. */
.app-main { flex: 1 1 auto; min-height: 0; display: flex; flex-direction: column; }
/* Override panel-kit's default `.ws-root { height: 100vh }` so the workspace
   fills the remaining space in `.app-main` instead of overflowing it. */
.app-main .ws-root { flex: 1 1 auto; min-height: 0; height: auto; min-width: 0; }
/* ...and the same fix for the tile tracks inside it. panel-kit projects the
   grid from the WINDOW viewport (its band is `100vh - 30`, the dock it
   reserves) and writes `grid-template-rows: repeat(n, <px>)` inline, but the
   workspace renders in `.app-main` — below the header and, on staging, the
   beta banner — which is exactly that much shorter. So the tracks were
   header-height taller than the screen: the signed-out Welcome panel measured
   478x1048 at y=41 (bottom 1089) on the canary's 1920x1080 viewport and
   overflowed it. Dropping the explicit row template lets each panel's own
   `grid-row` placement create implicit tracks that share the band actually on
   screen, and `minmax(0, …)` keeps a long panel from growing the row back —
   `.panel-body` scrolls instead (panel-kit.css). Columns stay inline:
   `.app-main` is exactly the window wide, so those tracks are already right.
   Left off `.compact`, where `.app-main` is `display: block` and the page —
   not the grid — is meant to scroll. */
.app-main .ws-root:not(.compact) .ws.tiling {
  grid-template-rows: none !important;
  grid-auto-rows: minmax(0, 1fr);
}

@media (max-width: 760px) {
  body { overflow-y: auto; overflow-x: hidden; height: auto; }
  .app-shell { height: auto; min-height: 100vh; overflow: visible; }
  .app-main { display: block; }
  .ws-root.compact { height: auto; }
  /* let stacked panels expand and the page scroll, instead of an inner scroll */
  .ws-root.compact .ws,
  .ws-root.compact .ws.tiling { overflow: visible; height: auto; }
}

/* ── Shared UI atoms (components/ui.rs) ─────────────────────────────────── */
.stat-tile { display: flex; flex-direction: column; gap: .25rem; padding: .75rem .9rem; min-width: 0; }
.stat-tile-label { font-family: var(--mono, monospace); font-size: var(--fs-2xs);
  text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary); }
.stat-tile-value { font-family: var(--mono, monospace); font-size: var(--fs-xl); font-weight: 700;
  font-variant-numeric: tabular-nums; line-height: 1.1; }
.stat-tile-sub { font-size: var(--fs-2xs); }

.section-tabs { display: flex; border: 1px solid var(--border-app); width: fit-content; max-width: 100%; }
.section-tab { padding: .4rem .8rem; background: var(--bg-card); color: var(--text-secondary); position: relative;
  border: none; cursor: pointer; font-family: var(--mono, monospace); font-size: var(--fs-2xs);
  text-transform: uppercase; letter-spacing: .05em; transition: color .15s ease, background .15s ease; }
.section-tab:hover { color: var(--text-primary); }
.section-tab + .section-tab { border-left: 1px solid var(--border-app); }
/* The strip stays visually compact; each tab's clickable area is 44px tall.
   The strip is one row at the top of its panel, so the expanded boxes reach
   into panel padding, never into another control. */
.section-tab::after { content: ""; position: absolute; left: 50%; top: 50%;
  transform: translate(-50%, -50%); width: max(100%, 44px); height: max(100%, 44px); }
/* Active tab is a selection fill, not an accent fill — same reason as
   .cw-focused: its meaning is "brighter than its siblings", so it takes
   --fill-yellow/--fill-ink rather than the pastel, which inverts in light. */
.section-tab-active, .section-tab-active:hover { background: var(--fill-yellow); color: var(--fill-ink); }
@media (max-width: 760px) { .section-tabs { width: 100%; display: flex; } .section-tab { flex: 1 1 0; } }

.rank-badge { width: 2rem; height: 2rem; border: 1px solid; font-weight: 700;
  display: inline-flex; align-items: center; justify-content: center;
  font-size: var(--fs-xs); font-family: var(--mono, monospace); flex-shrink: 0; }

/* Visually hidden, still in the accessibility tree. `display: none` and
   `visibility: hidden` both REMOVE the node from the AX tree, which is exactly
   the gap this exists to close (DEF-215), so the text is clipped instead of
   suppressed. */
.cw-sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; border: 0; }

.square-pulse { display: grid; grid-template-columns: repeat(5, 6px); gap: 3px; width: fit-content; }
.square-pulse-cell { width: 6px; height: 6px; background: var(--text-secondary);
  animation: square-pulse 1.2s ease-in-out infinite; }
@keyframes square-pulse { 0%, 100% { opacity: .15; } 50% { opacity: 1; } }
@media (prefers-reduced-motion: reduce) { .square-pulse-cell { animation: none; opacity: .6; } }

/* ── Reduced motion ──────────────────────────────────────────────────────────
   One guard for every animated surface in the app.

   PRINCIPLE: reduced motion must not reduce INFORMATION. Every transition
   here animates a state change that is also conveyed statically (a hovered
   button is still --color-primary-bordered, a selected cell is still
   --fill-yellow), so a transition becomes instant rather than absent. The
   infinite loops carry no state at all — decorative attention — so they stop,
   but each keeps a static read that still says "in progress" instead of
   collapsing to a blank or, worse, to something reading as "done".

   Follows the two existing guards in this codebase (.square-pulse-cell above,
   .gp-bar-fill in generation_progress.rs) and panel-kit's precedent at
   panel-kit.css:242-245, which holds an indeterminate bar at a static 40%
   sliver because a full-width bar would claim completion.

   WHY `!important` IS REQUIRED HERE
   ─────────────────────────────────
   This block lives in DESIGN, which main.rs:92 injects *before* the Router.
   Every component and page emits its own `<style>` from inside that Router
   (header.rs:152, pro_upgrade.rs:164, generation_progress.rs:184, the pages),
   so in document order this stylesheet comes FIRST. At equal specificity the
   later rule wins, so a plain declaration here loses to the component's own
   `transition:`/`animation:` — every declaration below that targets a
   component-local selector would be silently inert, including both infinite
   loops. Measured in Chromium: with plain declarations the block leaves 14 of
   the app's 20 animated surfaces still moving. `!important` is the correct
   tool: a user preference must outrank component styling, and it is already
   the established mechanism in this codebase (generation_progress.rs:327).
   The four selectors whose CSS is in this same file (body, .app-btn,
   .app-input, .section-tab) do not strictly need it, but it is applied
   uniformly so the block has one rule rather than two. */
@media (prefers-reduced-motion: reduce) {
  /* Transitions → instant. The end state is unchanged; only the
     interpolation goes. `body` is the theme-toggle cross-fade. */
  body,
  .app-btn,
  .app-input,
  .section-tab,
  .site-header .navlink,
  .game-row,
  .games-continue-card,
  .home-resume-card,
  .home-daily-card,
  .cg-rank-card,
  .cw-letter,
  .st-table-row,
  .gp-bar-fill,
  .st-bar-segment { transition: none !important; }

  /* Transforms that reflow surrounding content: keep the colour change,
     drop the scale. `transition: none` alone would make the 2% scale instant,
     which removes the motion but keeps the reflow on hover. */
  .site-header .brand svg,
  .pro-upgrade .upgrade-btn { transition: none !important; transform: none !important; }

  /* Infinite loops → static mid-opacity read. `.6` matches the
     .square-pulse-cell guard above. */
  .session-skeleton { animation: none !important; opacity: .6 !important; }

  /* The co-op socket's status pill (DEF-175 §5). A warning pill breathes at
     2.4s. Under `reduce` it renders static — a status indicator that demands
      attention through motion is wrong for the users who have asked for least
     motion, and the text alone already carries the whole message. The stale
     dimming is a `color-mix` on a non-text decoration, not an animation, so it
     needs no guard. */
  .cw-conn-pill { animation: none !important; opacity: 1 !important; }

  /* The Pro upsell spinner: freeze the rotation, keep the 3/4 ring so it
     still reads as a spinner rather than a static dot. */
  .pro-upgrade .spin-ring { animation: none !important; }

  /* Indeterminate progress: a static partial sliver. Full scale would
     claim completion; 0 would claim nothing is happening. 0.4 matches
     panel-kit's own .pk-progress-fill.indeterminate precedent. `!important`
     is required because the transform is an inline style
     (generation_progress.rs) and is safe because .gp-indeterminate is only
     ever set on indeterminate bars. */
  .gp-bar-fill.gp-indeterminate { animation: none !important; transform: scaleX(0.4) !important; }
}

/* ── Modal + drawer (components/ui.rs) ──────────────────────────────────── */
.modal-scrim, .drawer-scrim {
  position: fixed; inset: 0; z-index: 200; background: var(--scrim);
  display: flex; align-items: center; justify-content: center;
}
.confirm-modal {
  border-color: var(--color-error); width: min(24rem, calc(100vw - 2rem));
  padding: 1.1rem 1.25rem; display: flex; flex-direction: column; gap: .75rem;
}
.confirm-modal-title { margin: 0; font-family: var(--mono, monospace); font-size: var(--fs-md);
  text-transform: uppercase; letter-spacing: .05em; }
.confirm-modal-body { margin: 0; font-size: var(--fs-xs); line-height: 1.6; }
.confirm-modal-actions { display: flex; justify-content: flex-end; gap: .5rem; }
.confirm-modal-danger { color: var(--color-error); border-color: var(--color-error); }
.confirm-modal-danger:hover { background: color-mix(in srgb, var(--color-error) 12%, transparent); }
.drawer-scrim { justify-content: flex-end; align-items: stretch; }
.drawer {
  width: min(380px, 92vw); background: var(--bg-card);
  border-left: 1px solid var(--border-app); display: flex; flex-direction: column;
}
.drawer-head {
  display: flex; align-items: center; justify-content: space-between;
  padding: .6rem .9rem; border-bottom: 1px solid var(--border-app);
}
.drawer-title { font-family: var(--mono, monospace); font-size: var(--fs-xs); font-weight: 700;
  text-transform: uppercase; letter-spacing: .06em; }
.drawer-close { background: none; border: none; color: var(--text-secondary); cursor: pointer;
  font-size: var(--fs-md); min-width: 44px; min-height: 44px; display: inline-flex;
  align-items: center; justify-content: center; }
.drawer-close:hover { color: var(--text-primary); }
.drawer-body { padding: .9rem; overflow-y: auto; display: flex; flex-direction: column; gap: .75rem; }

/* ── Toasts (components/ui.rs ToastHost) ────────────────────────────────── */
.toast-host { position: fixed; top: 3.5rem; right: 1rem; z-index: 300;
  display: flex; flex-direction: column; gap: .5rem; max-width: 22rem; pointer-events: none; }
.toast { pointer-events: auto; cursor: pointer; background: var(--bg-card);
  border: 1px solid var(--border-app);
  /* Severity is carried by the 1px left edge, not a 3px side-tab. A thick
     coloured border down one side is the most recognisable tell of a
     generated UI, and at 1px the accent still reads as severity while the
     toast stops shouting over the page it is reporting on. The colour is
     reinforced by the toast's own text so severity never rests on hue alone. */
  border-left: 1px solid var(--text-secondary);
  padding: .6rem .9rem; font-family: var(--mono, monospace); font-size: var(--fs-xs); line-height: 1.5; }
.toast-error { border-left-color: var(--color-error); }
.toast-success { border-left-color: var(--color-success); }
.toast-warning { border-left-color: var(--color-warning); }
@media (max-width: 760px) {
  .toast-host { top: auto; bottom: 1rem; left: 1rem; right: 1rem; max-width: none; }
}

/* Shared layout helpers used across pages (lazy stand-ins for Tailwind utils). */
.container { max-width: 64rem; margin: 0 auto; padding: 1.5rem; }
.row { display: flex; gap: .75rem; align-items: center; }
.col { display: flex; flex-direction: column; gap: .75rem; }
.muted { color: var(--text-secondary); }
/* Centred loading / empty / error message, used by every list and detail panel. */
.game-status { padding: 2.5rem 1.5rem; text-align: center; font-size: var(--fs-xs);
  font-family: var(--mono, monospace); line-height: 1.6; }
/* ── Play screen pre-board states (DEF-195, spec DEF-180) ───────────────────
   The play screen's first paint used to be a bare <p class="muted">, and its
   error branch a bare <h1> + <p> + <Link>. Both are what a user sees first
   when they open a game, and both were the only surfaces in the app with no
   card around them. They share one card now, and the card is sized to the
   ERROR state in both: the body slot reserves its space with a min-height and
   the recovery row is always in the DOM, so the card cannot resize when the
   state flips. That is DEF-183 D3's remedy, measured there at CLS 0.056 on
   the boot card (122.8px loading -> 294.0px failed). */
/* Fills .app-main rather than claiming 100dvh: the play screen renders below
   the site header, so a 100dvh box that starts under it would centre the card
   half a header too low AND make the app shell's fixed height scroll — the
   .app-shell/.app-main flex pair (styles.rs:280-283) is the app's own #boot
   pattern, with the header subtracted. */
.gp-status { flex: 1 1 auto; display: flex; align-items: center; justify-content: center; padding: 1.5rem; }
.gp-status-card { width: 100%; max-width: 26rem; padding: 1.25rem;
  display: flex; flex-direction: column; gap: .75rem; }
/* The card border stays .app-card's --border-app (1.19:1 dark). That is the
   app-wide boundary, not a control: the card is a decorative box and the text
   inside carries the meaning. The controls inside (the .app-btn recovery link)
   are a different story and get --text-secondary. */
/* `--text-primary` explicitly, not inherited: `body` cross-fades `color` over
   .15s (styles.rs:213), so an inherited title is mid-transition for the first
   frames of a theme flip — which is exactly when a contrast check reads it. */
.gp-status-title { margin: 0; font-size: 1rem; font-weight: 700; color: var(--text-primary); }
/* One slot, never two rows. The bar and the detail swap inside it, and its
   min-height — three lines of .75rem/1.6 at the 13px root panel-kit sets —
   is what makes the loading and error cards exactly the same height. Without
   it a one-line "Game not found" would leave the card 28px shorter than the
   failure that carries a URL. */
.gp-status-body { min-height: 3.6rem; display: flex; flex-direction: column; justify-content: center; }
.gp-status-detail { margin: 0; font-size: var(--fs-xs); line-height: 1.6; color: var(--text-secondary);
  /* A network failure carries a URL, so it wraps; without this the card is
     wider than the viewport at 360px. The boot card's .boot-err rule. */
  overflow-wrap: anywhere; }
.gp-status-actions { display: flex; gap: .5rem; flex-wrap: wrap; }
/* Keyed off the ARIA state so the visual and the announced state cannot
   disagree: a `visibility: hidden` row keeps its box (which is the point — the
   space is reserved before the user ever needs it) but leaves the tab order,
   so the loading card has no tab stop and the error card has exactly one. */
.gp-status-actions[aria-hidden="true"] { visibility: hidden; }
.error { color: var(--color-error); }
.success { color: var(--color-success); }
"#;

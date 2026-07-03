//! App design tokens, ported from `assets/css/tailwind.css`. Injected once at
//! the app root, layered after `panel_kit::CSS`. The `.light` class on
//! `<html>` flips the theme (toggled by the header, persisted to localStorage).
//!
//! The second block remaps panel-kit's own variables onto these tokens so the
//! panel chrome on the play screen matches the rest of the app.

pub const DESIGN: &str = r#"
@import url('https://fonts.googleapis.com/css2?family=Inconsolata:wght@400;700&family=Montserrat:ital,wght@0,400;0,500;0,600;0,700;0,800;1,400&display=swap');

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
  --podium-silver: #cbd5e1;
  --podium-bronze: #d97706;

  /* App fonts. --mono is defined in the panel-kit remap block below. */
  --font-sans: 'Montserrat', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;

  /* Type scale (rem) + weights are numeric. Inline font-sizes scattered across
     components should adopt these vars over time; shared classes use them now. */
  --fs-2xs: .625rem;
  --fs-xs: .75rem;
  --fs-sm: .8rem;
  --fs-md: .875rem;
  --fs-lg: 1rem;
  --fs-xl: 1.5rem;
  --fs-2xl: 2rem;
}
.light-mode {
  --bg-app: #ffffff;
  --bg-card: #f4f4f5;
  --bg-cell-empty: #e4e4e7;
  --bg-cell-letter: #eaeaea;
  --text-primary: #18181b;
  --text-secondary: #71717a;
  --border-app: #e4e4e7;
  --border-hover: #d4d4d8;
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
  /* Min panel size so tiling never squeezes a panel small enough to clip its
     content (panel-kit reads these for both floating and tiling). */
  --panel-min-w: 340px;
  --panel-min-h: 240px;
}

/* In tiling mode, cap panel height to the workspace so long content (e.g. the
   leaderboard) scrolls inside the panel body instead of growing the panel and
   pushing the page. (Mobile keeps its stacked, page-scrolling behavior.) */
.ws-root:not(.mobile) .ws.tiling .panel { max-height: 100%; }

* { box-sizing: border-box; }
body {
  margin: 0;
  background-color: var(--bg-app);
  color: var(--text-primary);
  font-family: 'Montserrat', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
  transition: background-color .15s ease, color .15s ease, border-color .15s ease;
}
a { color: inherit; text-decoration: none; }

::-webkit-scrollbar { width: 4px; height: 4px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb { background: var(--border-app); border-radius: 0; }
::-webkit-scrollbar-thumb:hover { background: var(--border-hover); }

.app-card { background-color: var(--bg-card); border: 1px solid var(--border-app); border-radius: 0; }
.app-btn { font-family: var(--font-sans); padding: .5rem .9rem; font-size: var(--fs-md); font-weight: 600;
  border: 1px solid var(--border-app); border-radius: 0; background-color: var(--bg-card); color: var(--text-secondary);
  transition: all .15s ease; cursor: pointer; }
.app-btn:hover { color: var(--text-primary); border-color: var(--border-hover); }
.app-btn:disabled { opacity: .5; cursor: not-allowed; }
.app-btn-active { color: var(--text-primary); border-color: var(--color-primary); }
.app-input { background-color: var(--bg-cell-empty); color: var(--text-primary); border: 1px solid var(--border-app);
  border-radius: 0; outline: none; padding: .4rem .6rem; transition: border-color .15s ease; }
.app-input:focus { border-color: var(--color-primary); }

/* panel-kit's "traffic light" window controls (`.light`) are pure-color circles
   with no text content (see panel-kit.css) — there is no font to match, so
   nothing to override here. Left as-is intentionally. */

/* ── App shell: header + per-view panel-kit workspace + footer ──────────────
   Every view is a panel-kit workspace. On DESKTOP the workspace fills the area
   between the (sticky) header and footer — panels are clamped to vw × the
   available vh, and the page itself never scrolls. On MOBILE (<760px, where
   panel-kit force-stacks panels) we clamp width to vw but let height scroll,
   so stacked panels flow down the page. */
.app-shell { display: flex; flex-direction: column; height: 100vh; overflow: hidden; }
/* Column so a page that renders chrome (e.g. AdminNav) above its workspace
   stacks vertically, with the workspace taking the remaining height. */
.app-main { flex: 1 1 auto; min-height: 0; display: flex; flex-direction: column; }
/* Override panel-kit's default `.ws-root { height: 100vh }` so the workspace
   fills the remaining space in `.app-main` instead of overflowing it. */
.app-main .ws-root { flex: 1 1 auto; min-height: 0; height: auto; min-width: 0; }

@media (max-width: 760px) {
  body { overflow-y: auto; overflow-x: hidden; height: auto; }
  .app-shell { height: auto; min-height: 100vh; overflow: visible; }
  .app-main { display: block; }
  .ws-root.mobile { height: auto; }
  /* let stacked panels expand and the page scroll, instead of an inner scroll */
  .ws-root.mobile .ws,
  .ws-root.mobile .ws.tiling { overflow: visible; height: auto; }
}

/* Shared layout helpers used across pages (lazy stand-ins for Tailwind utils). */
.container { max-width: 64rem; margin: 0 auto; padding: 1.5rem; }
.row { display: flex; gap: .75rem; align-items: center; }
.col { display: flex; flex-direction: column; gap: .75rem; }
.muted { color: var(--text-secondary); }
.error { color: var(--color-error); }
.success { color: var(--color-success); }
"#;

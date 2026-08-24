//! App design tokens, ported from `assets/css/tailwind.css`. Injected once at
//! the app root, layered after `panel_kit::CSS`. The `.light-mode` class on
//! `<html>` flips the theme (toggled by the header, persisted to localStorage).
//! Not `.light` — panel-kit owns that for its traffic-light dots; see
//! `main::set_light_class`.
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
  /* Min panel size so tiling never squeezes a panel small enough to clip its
     content (panel-kit reads these for both floating and tiling). */
  --panel-min-w: 340px;
  --panel-min-h: 240px;
}

/* In tiling mode, cap panel height to the workspace so long content (e.g. the
   leaderboard) scrolls inside the panel body instead of growing the panel and
   pushing the page. (Mobile keeps its stacked, page-scrolling behavior.) */
.ws-root:not(.mobile) .ws.tiling .panel { max-height: 100%; }

/* panel-kit hardcodes its drop shadows in literal black, which no theme
   variable reaches, so the panel chrome kept a shadow tuned for a near-black
   workspace even in light mode. main.rs injects DESIGN *after* panel_kit::CSS,
   so these equal-specificity re-declarations win the cascade — cheaper and
   safer than forking a crate with other consumers. Only the shadow (and the
   tooltip's surface) is restated; all geometry stays panel-kit's. */
.panel { box-shadow: var(--shadow-panel); }
/* .tip-overlay also hardcodes `background:#0d0d0d` while inheriting `color`
   from --fg. In light mode that is near-black text on a near-black card, i.e.
   an unreadable tooltip; pin it to the themed surface instead. */
.tip-overlay { background: var(--panel); color: var(--fg); box-shadow: var(--shadow-pop); }

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

/* ── Shared UI atoms (components/ui.rs) ─────────────────────────────────── */
.stat-tile { display: flex; flex-direction: column; gap: .25rem; padding: .75rem .9rem; min-width: 0; }
.stat-tile-label { font-family: var(--mono, monospace); font-size: var(--fs-2xs);
  text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary); }
.stat-tile-value { font-family: var(--mono, monospace); font-size: var(--fs-xl); font-weight: 700;
  font-variant-numeric: tabular-nums; line-height: 1.1; }
.stat-tile-sub { font-size: var(--fs-2xs); }

.section-tabs { display: flex; border: 1px solid var(--border-app); width: fit-content; max-width: 100%; }
.section-tab { padding: .4rem .8rem; background: var(--bg-card); color: var(--text-secondary);
  border: none; cursor: pointer; font-family: var(--mono, monospace); font-size: var(--fs-2xs);
  text-transform: uppercase; letter-spacing: .05em; transition: color .15s ease, background .15s ease; }
.section-tab:hover { color: var(--text-primary); }
.section-tab + .section-tab { border-left: 1px solid var(--border-app); }
/* Active tab is a selection fill, not an accent fill — same reason as
   .cw-focused: its meaning is "brighter than its siblings", so it takes
   --fill-yellow/--fill-ink rather than the pastel, which inverts in light. */
.section-tab-active, .section-tab-active:hover { background: var(--fill-yellow); color: var(--fill-ink); }
@media (max-width: 760px) { .section-tabs { width: 100%; display: flex; } .section-tab { flex: 1 1 0; } }

.rank-badge { width: 2rem; height: 2rem; border: 1px solid; font-weight: 700;
  display: inline-flex; align-items: center; justify-content: center;
  font-size: var(--fs-xs); font-family: var(--mono, monospace); flex-shrink: 0; }

.square-pulse { display: grid; grid-template-columns: repeat(5, 6px); gap: 3px; width: fit-content; }
.square-pulse-cell { width: 6px; height: 6px; background: var(--text-secondary);
  animation: square-pulse 1.2s ease-in-out infinite; }
@keyframes square-pulse { 0%, 100% { opacity: .15; } 50% { opacity: 1; } }
@media (prefers-reduced-motion: reduce) { .square-pulse-cell { animation: none; opacity: .6; } }

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
  font-size: var(--fs-md); padding: .15rem .35rem; }
.drawer-close:hover { color: var(--text-primary); }
.drawer-body { padding: .9rem; overflow-y: auto; display: flex; flex-direction: column; gap: .75rem; }

/* ── Toasts (components/ui.rs ToastHost) ────────────────────────────────── */
.toast-host { position: fixed; top: 3.5rem; right: 1rem; z-index: 300;
  display: flex; flex-direction: column; gap: .5rem; max-width: 22rem; pointer-events: none; }
.toast { pointer-events: auto; cursor: pointer; background: var(--bg-card);
  border: 1px solid var(--border-app); border-left: 3px solid var(--text-secondary);
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
.error { color: var(--color-error); }
.success { color: var(--color-success); }
"#;

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
}
.light {
  --bg-app: #ffffff;
  --bg-card: #f4f4f5;
  --bg-cell-empty: #e4e4e7;
  --bg-cell-letter: #eaeaea;
  --text-primary: #18181b;
  --text-secondary: #71717a;
  --border-app: #e4e4e7;
  --border-hover: #d4d4d8;
}

/* Map panel-kit's variables onto the app tokens. */
:root {
  --bg: var(--bg-card);
  --fg: var(--text-primary);
  --dim: var(--text-secondary);
  --line: var(--border-app);
  --line2: var(--border-hover);
  --accent: var(--color-primary);
  --mono: 'Inconsolata', ui-monospace, monospace;
}

* { box-sizing: border-box; }
/* Square corners globally — house style. Overrides panel-kit + component CSS. */
*, *::before, *::after { border-radius: 0 !important; }
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
::-webkit-scrollbar-thumb { background: var(--border-app); border-radius: 2px; }
::-webkit-scrollbar-thumb:hover { background: var(--border-hover); }

.app-card { background-color: var(--bg-card); border: 1px solid var(--border-app); border-radius: .75rem; }
.app-btn { padding: .375rem .75rem; font-size: .875rem; font-weight: 500; border: 1px solid var(--border-app);
  border-radius: .375rem; background-color: var(--bg-card); color: var(--text-secondary);
  transition: all .15s ease; cursor: pointer; }
.app-btn:hover { color: var(--text-primary); border-color: var(--border-hover); }
.app-btn:disabled { opacity: .5; cursor: not-allowed; }
.app-btn-active { color: var(--text-primary); border-color: var(--color-primary); }
.app-input { background-color: var(--bg-cell-empty); color: var(--text-primary); border: 1px solid var(--border-app);
  border-radius: .375rem; outline: none; padding: .4rem .6rem; transition: border-color .15s ease; }
.app-input:focus { border-color: var(--color-primary); }

/* Shared layout helpers used across pages (lazy stand-ins for Tailwind utils). */
.container { max-width: 64rem; margin: 0 auto; padding: 1.5rem; }
.row { display: flex; gap: .75rem; align-items: center; }
.col { display: flex; flex-direction: column; gap: .75rem; }
.muted { color: var(--text-secondary); }
.error { color: var(--color-error); }
.success { color: var(--color-success); }
"#;

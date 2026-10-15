/**
 * heal-reporter — the deterministic half of the "Follow-up: AI self-heal" phase
 * described in `e2e/README.md`:
 *
 *   "Semantic locators already survive most UI change. A future phase can add a
 *    cheap LLM fallback that, when a locator genuinely breaks, recovers the step
 *    and posts a 'test needs updating' suggestion — cutting maintenance further
 *    without making the canary non-deterministic (AI only runs on a real break,
 *    never on the happy path)."
 *
 * This file is that phase's first half and nothing more: when a test genuinely
 * fails, it dumps a small, redacted JSON+Markdown "heal bundle" next to that
 * test's own artifacts, containing enough context (error, stack, failing call,
 * the spec source around the failing line) for either a human or a cheap LLM pass
 * to propose the replacement step.
 *
 * Design rules this file obeys, deliberately:
 *
 *  1. NO LLM CALLS. There is no model call anywhere in this reporter, and there
 *     will not be one. CI must never need an API key to run the canary. The LLM
 *     pass is a *separate, opt-in consumer* of the bundle that a human or a
 *     later job runs — see `e2e/HEAL.md`.
 *  2. INERT ON THE HAPPY PATH. On a fully green run this reporter writes
 *     nothing, reads no spec source, and does no work beyond counting results
 *     for the one-line summary. It is gated behind `E2E_HEAL=1` *and* behind
 *     `result.status === 'failed'`, so with the env var unset the default
 *     behaviour is byte-for-byte today's behaviour: a failing test fails, a
 *     passing test passes.
 *  3. MANDATORY REDACTION. A heal bundle gets attached to CI artifacts and
 *     pasted into issues. A leaked `next-auth.session-token` is a *live
 *     credential* that anyone reading the issue could replay. Every string that
 *     goes into the bundle passes through `redact()` first: session cookies,
 *     `Cookie:`/`Authorization:` header values, credentials embedded in URLs,
 *     and anything that looks like `SOME_SECRET_VAR=value`. On top of the
 *     pattern sweep we also substitute the literal values of any known-secret
 *     env var present in the reporter process. This is deliberately not a DLP —
 *     see the "limits" section of `e2e/HEAL.md` for what that means in practice.
 *  4. NO HARDCODED OUTPUT PATHS. Bundles are written under the failing test's
 *     own output directory (i.e. wherever `--output` points), so a retry attempt
 *     cannot clobber the previous attempt's bundle.
 */

import * as fs from "fs";
import * as path from "path";
import type {
  FullConfig,
  FullResult,
  Reporter,
  Suite,
  TestCase,
  TestResult,
} from "@playwright/test/reporter";

/** Lines of stack kept in the bundle. */
const STACK_LINES = 40;
/** Lines of spec source kept on each side of the failing line. */
const SOURCE_RADIUS = 20;
/** Gate: without this, the reporter is inert. */
const ENABLE_ENV = "E2E_HEAL";

const REDACTED = "<redacted>";

/**
 * Env vars whose *values* are scrubbed from the bundle whenever they happen to
 * appear as literal text (a cookie header dumped into an error message, say).
 * The pattern sweep below catches `NAME=value` shapes; this catches the case
 * where only the bare secret is present.
 */
const SECRET_VALUE_ENVS = [
  "E2E_PASSWORD",
  "E2E_PASSWORD_2",
  "E2E_PASSWORD_3",
  "E2E_PASSWORD_4",
  "E2E_USERS_JSON",
  "DISCORD_WEBHOOK",
  "SOPS_AGE_KEY",
];

const SECRET_VAR_NAME = "[A-Za-z0-9_]*(?:SECRET|TOKEN|PASSWORD|PASSWD|PASS|API_?KEY|PRIVATE_KEY|CREDENTIAL|COOKIE|SESSION)[A-Za-z0-9_]*";

/** Ordered redaction rules; first match wins per position. */
const REDACTIONS: Array<[RegExp, string]> = [
  // next-auth / session cookies, in any `name=value` or bare-token form.
  [
    /(next-auth\.[A-Za-z0-9_-]*session-token)(\s*=\s*)([^\s;,'"\\]+)/gi,
    `$1$2${REDACTED}`,
  ],
  // Any cookie-ish pair whose *name* looks like a session.
  [
    /((?:__Secure-)?(?:session|sess|sid|auth|jwt|csrf)[A-Za-z0-9_-]*)(\s*=\s*)([^\s;,'"\\]+)/gi,
    `$1$2${REDACTED}`,
  ],
  // `Cookie: ...` / `Set-Cookie: ...` header values — whole header value.
  [
    /((?:set-)?cookie\s*:\s*)([^\r\n]*)/gi,
    `$1${REDACTED}`,
  ],
  // `Authorization: Bearer xyz` / `Authorization: Basic xyz`.
  [
    /(authorization\s*:\s*)([^\r\n,}]+)/gi,
    `$1${REDACTED}`,
  ],
  // Credentials embedded in a URL: scheme://user:pass@host
  [
    /(\b[a-z][a-z0-9+.-]*:\/\/)([^\s:/@]+):([^\s@/]+)@/gi,
    `$1${REDACTED}:${REDACTED}@`,
  ],
  // `SOME_SECRET_VAR=value` / `SOME_SECRET_VAR: value`.
  [
    new RegExp(`(${SECRET_VAR_NAME})(\\s*[:=]\\s*)("[^"]*"|'[^']*'|[^\\s,;}"']+)`, "g"),
    `$1$2${REDACTED}`,
  ],
];

/**
 * Scrub one string. Pure, idempotent, and total: it never throws, because a
 * reporter that throws would be swallowed by Playwright and silently lose the
 * bundle it was asked to write.
 */
export function redact(input: string): string {
  let out = input;
  for (const name of SECRET_VALUE_ENVS) {
    const value = process.env[name];
    if (value && value.length >= 8) {
      out = out.split(value).join(REDACTED);
    }
  }
  for (const [pattern, replacement] of REDACTIONS) {
    out = out.replace(pattern, replacement);
  }
  return out;
}

/** Redact every string in a JSON-shaped value, in place, by rebuilding it. */
export function redactDeep<T>(value: T): T {
  if (typeof value === "string") return redact(value) as unknown as T;
  if (Array.isArray(value)) return value.map((v) => redactDeep(v)) as unknown as T;
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      out[k] = redactDeep(v);
    }
    return out as unknown as T;
  }
  return value;
}

/** Shape written to `heal-bundle.json`. Stable: an LLM consumer keys off these. */
interface HealBundle {
  version: 1;
  generatedAt: string;
  test: {
    title: string;
    titlePath: string[];
    file: string;
    project: string;
    line: number;
    column: number;
  };
  failure: {
    status: string;
    message: string;
    stack: string[];
    failingLine: number | null;
    failingCall: string | null;
    locatorHints: string[];
  };
  run: {
    durationMs: number;
    retry: number;
    retryOf?: number;
    totalRetries: number;
    attachments: Array<{ name: string; contentType: string; path: string | null }>;
  };
  specSource: {
    file: string;
    startLine: number;
    endLine: number;
    lines: Array<{ line: number; text: string; isFailing: boolean }>;
  };
  suggestedNextStep: string;
}

function enabled(): boolean {
  const raw = process.env[ENABLE_ENV];
  return raw === "1" || raw?.toLowerCase() === "true";
}

/**
 * Best-effort per-test output directory. Playwright's reporter API does not hand
 * us `testInfo.outputDir` directly, but it does hand us attachment paths, which
 * live in exactly that directory. Fall back to the run's `--output` dir plus the
 * per-test id (which is what Playwright itself uses) when nothing attached.
 */
function resolveOutputDir(test: TestCase, result: TestResult): string {
  // Authoritative when anything attached: attachment paths live in exactly
  // `testInfo.outputDir`. With `screenshot: "only-on-failure"` there is always
  // at least one on a failure.
  for (const attachment of result.attachments) {
    if (attachment.path) return path.dirname(attachment.path);
  }
  // Otherwise reconstruct Playwright's own layout under the run's `--output`
  // dir (`FullProject.outputDir` is where that lands; never a hardcoded path).
  const project = test.parent.project();
  const base = project?.outputDir ?? path.resolve(process.cwd(), "test-results");
  const name = (project?.name ?? "").replace(/[^\w.-]+/g, "_");
  return path.join(base, name ? `${test.id}-${name}` : test.id);
}

/**
 * The failing line number, derived from the stack frame that points back into
 * the spec. `test.location` is the *declaration* line; the break is elsewhere.
 */
function findFailingLine(
  stack: string | undefined,
  specFile: string,
): number | null {
  if (!stack) return null;
  const needle = path.basename(specFile).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  for (const line of stack.split("\n")) {
    const match = line.match(new RegExp(`${needle}:(\\d+):(\\d+)`));
    if (match) return Number(match[1]);
  }
  return null;
}

/**
 * The Playwright call that failed, as far as it can be derived from the message
 * and the top spec stack frame. Deterministic string work only — no evaluation,
 * no heuristics that could vary run to run.
 */
function findFailingCall(message: string, stack: string[]): string | null {
  for (const line of stack) {
    if (line.trim().startsWith("at ") && !line.includes("node_modules")) return line.trim();
  }
  const first = message.split("\n")[0]?.trim();
  return first ? first : null;
}

/** Locator-ish substrings quoted in the error message: `getByRole('button', …)`. */
function findLocatorHints(message: string): string[] {
  // Call heads only: `page.getByRole(`, `locator.click(`, `expect(` … Take the
  // text up to the first `)` (bounded) rather than trying to balance quotes —
  // Playwright call text routinely nests quotes and braces.
  const hints = new Set<string>();
  const callHead =
    /\b(?:page|locator|frame|expect)\s*\.\s*[A-Za-z]+\s*\(|\bgetBy[A-Za-z]+\s*\(|\bexpect\s*\(/g;
  for (const match of message.matchAll(callHead)) {
    const tail = message.slice(match.index + match[0].length, match.index + 140);
    const end = tail.indexOf(")");
    hints.add(`${match[0]}${end === -1 ? tail : tail.slice(0, end + 1)}`.replace(/\s+/g, " "));
  }
  return [...hints].slice(0, 10);
}

/** Read the spec around the failing line so the healer sees intent, not just break. */
function readSpecSource(
  specFile: string,
  failingLine: number | null,
): HealBundle["specSource"] {
  const fallback: HealBundle["specSource"] = {
    file: specFile,
    startLine: 0,
    endLine: 0,
    lines: [],
  };
  let raw: string;
  try {
    raw = fs.readFileSync(specFile, "utf8");
  } catch {
    return fallback;
  }
  const all = raw.split("\n");
  const anchor = failingLine ?? 1;
  const start = Math.max(1, anchor - SOURCE_RADIUS);
  const end = Math.min(all.length, anchor + SOURCE_RADIUS);
  const lines: Array<{ line: number; text: string; isFailing: boolean }> = [];
  for (let n = start; n <= end; n++) {
    lines.push({ line: n, text: redact(all[n - 1] ?? ""), isFailing: n === anchor });
  }
  return { file: specFile, startLine: start, endLine: end, lines };
}

/**
 * The deterministic instruction handed to the reader. No model, no guesswork:
 * if this string is wrong, a human fixes a string.
 */
function suggestedNextStep(ref: {
  file: string;
  failingLine: number | null;
  declaredLine: number;
  title: string;
  locatorHint: string | undefined;
}): string {
  const loc = `${ref.file}:${ref.failingLine ?? ref.declaredLine}`;
  return (
    `Locator broke (not an app bug until proven otherwise). Do NOT relax the assertion. ` +
    `1) Re-run this spec headed: \`npx playwright test ${path.relative(process.cwd(), ref.file)} --headed --trace on\`. ` +
    `2) Open the trace for ${ref.title} (\`npx playwright show-trace <trace.zip>\`, same output dir) and find the step that timed out or resolved to the wrong node. ` +
    `3) Update the selector in ${loc} — prefer a semantic locator (getByRole / getByLabel / getByText) over CSS or testids. ` +
    (ref.locatorHint ? `Suspect call: ${ref.locatorHint}. ` : "") +
    `4) If the UI genuinely regressed, file that instead — do not "heal" a real bug.`
  );
}

export default class HealReporter implements Reporter {
  private ran = 0;
  private readonly bundlePaths: string[] = [];
  private config!: FullConfig;

  onBegin(config: FullConfig, _suite: Suite): void {
    this.config = config;
  }

  onTestEnd(test: TestCase, result: TestResult): void {
    if (!enabled()) return;
    this.ran++;
    // Happy path: a pass writes nothing, reads nothing, allocates nothing.
    if (result.status !== "failed" || !result.error) return;

    try {
      this.writeBundle(test, result);
    } catch (error) {
      // Playwright swallows reporter errors; make the failure visible instead.
      process.stderr.write(
        `heal-reporter: failed to write bundle for ${test.title}: ${
          error instanceof Error ? error.message : String(error)
        }\n`,
      );
    }
  }

  onEnd(_result: FullResult): void {
    if (!enabled()) return;
    const bundles = this.bundlePaths.length;
    const rendered = this.bundlePaths
      .map((p) => path.relative(process.cwd(), p))
      .join(", ");
    process.stdout.write(
      `heal-reporter: ${this.ran} test(s) ran, ${bundles} heal bundle(s)${
        rendered ? ` -> ${rendered}` : ""
      }\n`,
    );
  }

  private writeBundle(test: TestCase, result: TestResult): void {
    const error = result.error!;
    const specFile = test.location.file;
    // A test can fail by throwing a non-Error value; `TestError.value` is then
    // the carrier. Normalize once so every downstream read is a plain string.
    const message = error.message ?? error.value ?? "";
    const stack = (error.stack ?? message).split("\n").slice(0, STACK_LINES);
    const failingLine = findFailingLine(error.stack, specFile);

    const bundle: HealBundle = {
      version: 1,
      generatedAt: new Date().toISOString(),
      test: {
        title: test.title,
        titlePath: test.titlePath(),
        file: specFile,
        project: test.parent.project()?.name || "<unknown>",
        line: test.location.line,
        column: test.location.column,
      },
      failure: {
        status: result.status,
        message,
        stack,
        failingLine,
        failingCall: findFailingCall(message, stack),
        locatorHints: findLocatorHints(message),
      },
      run: {
        durationMs: result.duration,
        retry: result.retry,
        totalRetries: test.retries,
        attachments: result.attachments.map((a) => ({
          name: a.name,
          contentType: a.contentType,
          path: a.path ?? null,
        })),
      },
      specSource: readSpecSource(specFile, failingLine),
      suggestedNextStep: suggestedNextStep({
        file: specFile,
        failingLine,
        declaredLine: test.location.line,
        title: test.title,
        locatorHint: findLocatorHints(message)[0],
      }),
    };
    if (result.retry > 0) bundle.run.retryOf = result.retry - 1;

    // Redaction is the last thing that touches the payload, and it is
    // unconditional — there is no code path that writes a raw bundle.
    const safe = redactDeep(bundle);

    const dir = resolveOutputDir(test, result);
    fs.mkdirSync(dir, { recursive: true });
    // Each attempt gets its own file name as well as (usually) its own
    // directory, so a retry can never clobber the first attempt's bundle even
    // when Playwright hands both attempts the same output dir.
    const stem = result.retry > 0 ? `heal-bundle-retry${result.retry}` : "heal-bundle";

    const jsonPath = path.join(dir, `${stem}.json`);
    fs.writeFileSync(jsonPath, JSON.stringify(safe, null, 2) + "\n", "utf8");

    const mdPath = path.join(dir, `${stem}.md`);
    fs.writeFileSync(mdPath, renderMarkdown(safe), "utf8");

    this.bundlePaths.push(jsonPath, mdPath);
  }
}

/** Short human-readable twin of the JSON bundle. Same redacted payload. */
function renderMarkdown(bundle: HealBundle): string {
  const lines: string[] = [
    `# Heal bundle — ${bundle.test.title}`,
    "",
    `> Generated by \`e2e/heal-reporter.ts\` when the test failed. Secrets are`,
    `> redacted (session cookies, \`Cookie:\`/\`Authorization:\` headers, secret-looking`,
    `> env vars). This file is an input to a suggestion — it is not a verdict.`,
    "",
    `- **Spec**: \`${bundle.test.file}:${bundle.failure.failingLine ?? bundle.test.line}\``,
    `- **Project**: ${bundle.test.project}`,
    `- **Status**: ${bundle.failure.status} (retry ${bundle.run.retry} of ${bundle.run.totalRetries})`,
    `- **Duration**: ${bundle.run.durationMs} ms`,
    "",
    "## Error",
    "",
    "```",
    ...bundle.failure.stack,
    "```",
    "",
    "## Spec source around the failure",
    "",
    "```ts",
    ...bundle.specSource.lines.map(
      (l) => `${l.isFailing ? ">" : " "} ${String(l.line).padStart(4)} | ${l.text}`,
    ),
    "```",
    "",
    "## Suggested next step",
    "",
    bundle.suggestedNextStep,
    "",
  ];
  return lines.join("\n");
}
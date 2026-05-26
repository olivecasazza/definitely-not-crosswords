<template>
  <div class="app-card flex flex-col gap-3 p-4">
    <div class="flex items-center justify-between border-b border-[var(--border-app)] pb-2">
      <h2 class="font-mono text-sm font-bold tracking-wider">
        <span v-if="status === 'running'">GENERATING…</span>
        <span v-else-if="status === 'succeeded'" class="text-[var(--color-success)]">GENERATION COMPLETE</span>
        <span v-else-if="status === 'failed'" class="text-[var(--color-error)]">GENERATION FAILED</span>
        <span v-else>GENERATION LOG</span>
      </h2>
      <span class="font-mono text-xs tabular-nums text-[var(--text-secondary)]">{{ elapsedLabel }}</span>
    </div>

    <!-- live progress bar (driven by the latest `progress` event) -->
    <div class="flex flex-col gap-1.5">
      <div class="flex items-center justify-between font-mono text-xs">
        <span class="uppercase tracking-wider text-[var(--text-secondary)]">{{ stageLabel }}</span>
        <span v-if="progress" class="tabular-nums text-[var(--text-secondary)]">
          {{ progress.current }} / {{ progress.total }} ({{ pct }}%)
        </span>
      </div>
      <div class="h-2 w-full overflow-hidden rounded bg-[var(--bg-cell-empty)]">
        <div
          class="h-full rounded transition-[width] duration-200 ease-out"
          :class="[barColor, { 'animate-pulse': indeterminate }]"
          :style="{ width: barWidth }"
        />
      </div>
      <p v-if="progress?.message" class="text-xs text-[var(--text-secondary)]">{{ progress.message }}</p>
    </div>

    <!-- event / debug feed -->
    <div
      ref="feedEl"
      class="flex max-h-56 flex-col gap-0.5 overflow-y-auto rounded bg-[var(--bg-cell-empty)] p-2 font-mono text-[11px] leading-relaxed"
    >
      <div v-for="(line, i) in lines" :key="i" class="flex gap-2" :class="line.class">
        <span class="shrink-0 tabular-nums text-[var(--text-secondary)]">{{ line.time }}</span>
        <span class="break-words">{{ line.text }}</span>
      </div>
      <div v-if="!lines.length" class="text-[var(--text-secondary)]">Waiting for events…</div>
    </div>
  </div>
</template>

<script setup lang="ts">
type LogEvent = {
  type: string;
  at: number;
  stage?: string;
  message?: string;
  level?: string;
  error?: string;
  title?: string;
  questionCount?: number;
};

type Progress = { stage: string; current: number; total: number; message?: string };

const props = defineProps<{
  log: LogEvent[];
  progress: Progress | null;
  running: boolean;
  startedAt: number | null;
  status: "idle" | "running" | "succeeded" | "failed";
}>();

const STAGE_LABELS: Record<string, string> = {
  "loading-dictionary": "Loading dictionary",
  "embedding-model": "Loading embedding model",
  embedding: "Embedding candidates",
  solving: "Solving grids",
  "solving-attempts": "Placing words",
  validating: "Validating grid",
};

const stageLabel = computed(() => {
  if (props.status === "succeeded") return "Done";
  if (props.status === "failed") return "Failed";
  if (props.progress) return STAGE_LABELS[props.progress.stage] ?? props.progress.stage;
  // fall back to the most recent stage event
  for (let i = props.log.length - 1; i >= 0; i--) {
    if (props.log[i].type === "stage") {
      const s = props.log[i].stage ?? "";
      return STAGE_LABELS[s] ?? s;
    }
  }
  return props.running ? "Starting…" : "Idle";
});

const pct = computed(() =>
  props.progress && props.progress.total
    ? Math.min(100, Math.round((props.progress.current / props.progress.total) * 100))
    : 0
);

// Running with no numeric progress yet (e.g. loading the model) → indeterminate.
const indeterminate = computed(() => props.running && !props.progress);
const barWidth = computed(() => {
  if (props.status === "succeeded") return "100%";
  if (indeterminate.value) return "100%";
  return pct.value + "%";
});
const barColor = computed(() => {
  if (props.status === "failed") return "bg-[var(--color-error)]";
  if (props.status === "succeeded") return "bg-[var(--color-success)]";
  return "bg-[var(--color-primary)]";
});

// live elapsed clock while running
const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | null = null;
watch(
  () => props.running,
  (running) => {
    if (running) {
      now.value = Date.now();
      timer ??= setInterval(() => (now.value = Date.now()), 250);
    } else if (timer) {
      clearInterval(timer);
      timer = null;
    }
  },
  { immediate: true }
);
onBeforeUnmount(() => timer && clearInterval(timer));

const elapsedLabel = computed(() => {
  if (!props.startedAt) return "";
  const end = props.running ? now.value : props.log.at(-1)?.at ?? now.value;
  const secs = Math.max(0, Math.round((end - props.startedAt) / 1000));
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return m ? `${m}m ${s.toString().padStart(2, "0")}s` : `${s}s`;
});

function fmtTime(at: number) {
  return new Date(at).toLocaleTimeString([], { hour12: false });
}

const lines = computed(() =>
  props.log.map((e) => {
    let text = "";
    let cls = "text-[var(--text-primary)]";
    switch (e.type) {
      case "started":
        text = "▶ generation started";
        cls = "text-[var(--text-secondary)]";
        break;
      case "stage":
        text = `■ ${STAGE_LABELS[e.stage ?? ""] ?? e.stage}${e.message ? ": " + e.message : ""}`;
        cls = "font-semibold text-[var(--text-primary)]";
        break;
      case "log":
        text = e.message ?? "";
        cls =
          e.level === "error"
            ? "text-[var(--color-error)]"
            : e.level === "warn"
              ? "text-[var(--pastel-yellow)]"
              : "text-[var(--text-secondary)]";
        break;
      case "completed":
        text = `✓ completed — ${e.questionCount ?? "?"} answers${e.title ? ` (“${e.title}”)` : ""}`;
        cls = "font-semibold text-[var(--color-success)]";
        break;
      case "failed":
        text = `✗ failed — ${e.error ?? "unknown error"}`;
        cls = "font-semibold text-[var(--color-error)]";
        break;
      default:
        text = e.message ?? e.type;
    }
    return { time: fmtTime(e.at), text, class: cls };
  })
);

// autoscroll the feed as events arrive
const feedEl = ref<HTMLElement | null>(null);
watch(
  () => props.log.length,
  async () => {
    await nextTick();
    if (feedEl.value) feedEl.value.scrollTop = feedEl.value.scrollHeight;
  }
);
</script>

<template>
  <main class="flex-grow p-6 w-full max-w-4xl mx-auto flex flex-col gap-6">
    <div class="app-card p-6 flex flex-col gap-6">
      <AdminNav />
      <div class="border-b border-[var(--border-app)] pb-4">
        <h1 class="text-lg font-bold font-primary tracking-wider">CROSSWORD GENERATOR</h1>
      </div>

      <div class="flex flex-col gap-4">
        <div class="flex flex-wrap items-end gap-3">
          <div class="flex min-w-[280px] flex-1 flex-col gap-1.5">
            <label class="text-xs font-semibold text-[var(--text-secondary)] uppercase tracking-wider" for="topic">Topic</label>
            <input
              id="topic"
              v-model="form.topic"
              class="app-input px-3 py-2 text-sm w-full"
              type="text"
            />
          </div>

          <button
            class="app-btn app-btn-active h-[38px] min-w-[120px] font-bold"
            :disabled="isGenerating || !user?.user?.email"
            @click="generate"
          >
            {{ isGenerating ? "Generating..." : "Generate" }}
          </button>
        </div>

        <div class="grid grid-cols-2 gap-3 md:grid-cols-6">
          <label class="flex flex-col gap-1 text-xs text-[var(--text-secondary)] uppercase tracking-wider">
            Width
            <input v-model.number="form.width" class="app-input px-2 py-1.5 text-sm" type="number" min="3" max="50" />
          </label>
          <label class="flex flex-col gap-1 text-xs text-[var(--text-secondary)] uppercase tracking-wider">
            Height
            <input v-model.number="form.height" class="app-input px-2 py-1.5 text-sm" type="number" min="3" max="50" />
          </label>
          <label class="flex flex-col gap-1 text-xs text-[var(--text-secondary)] uppercase tracking-wider">
            Min Len
            <input v-model.number="form.minWordLength" class="app-input px-2 py-1.5 text-sm" type="number" min="2" max="50" />
          </label>
          <label class="flex flex-col gap-1 text-xs text-[var(--text-secondary)] uppercase tracking-wider">
            Max Len
            <input v-model.number="form.maxWordLength" class="app-input px-2 py-1.5 text-sm" type="number" min="2" max="50" />
          </label>
          <label class="flex flex-col gap-1 text-xs text-[var(--text-secondary)] uppercase tracking-wider">
            Answers
            <input v-model.number="form.targetWords" class="app-input px-2 py-1.5 text-sm" type="number" min="1" max="250" />
          </label>
          <label class="flex flex-col gap-1 text-xs text-[var(--text-secondary)] uppercase tracking-wider">
            Runs
            <input v-model.number="form.runs" class="app-input px-2 py-1.5 text-sm" type="number" min="1" max="100" />
          </label>
        </div>
      </div>
    </div>

    <GenerationProgress
      v-if="genStatus !== 'idle'"
      :log="genLog"
      :progress="genProgress"
      :running="isGenerating"
      :started-at="genStartedAt"
      :status="genStatus"
    />

    <div v-if="errorMessage" class="app-card border-[var(--color-error)] bg-[var(--color-error)]/10 p-4 text-sm text-[var(--color-error)]">
      {{ errorMessage }}
    </div>

    <div v-if="generatedGame" class="app-card border-[var(--color-success)] bg-[var(--color-success)]/10 p-4 flex flex-row items-center justify-between">
      <div>
        <div class="text-sm font-semibold">{{ generatedGame.title }}</div>
        <div class="text-xs text-[var(--text-secondary)] mt-0.5">
          {{ generatedGame.questions.length }} answers saved
        </div>
      </div>
      <div class="flex gap-2">
        <button
          v-if="!generatedGame.published"
          class="app-btn app-btn-active font-bold"
          :disabled="publishingGameId === generatedGame.id"
          @click="publishGame(generatedGame.id)"
        >
          {{ publishingGameId === generatedGame.id ? "Publishing..." : "Publish" }}
        </button>
        <button class="app-btn" @click="navigateTo('/games')">
          View Games
        </button>
      </div>
    </div>

    <div class="app-card overflow-hidden">
      <div class="p-4 border-b border-[var(--border-app)] flex items-center justify-between">
        <h2 class="text-sm font-bold font-mono tracking-wider">GENERATION JOBS</h2>
        <button class="app-btn text-xs font-mono uppercase" :disabled="isLoadingJobs" @click="refreshJobs">
          {{ isLoadingJobs ? "Refreshing" : "Refresh" }}
        </button>
      </div>

      <div v-if="jobsError" class="border-b border-[var(--border-app)] bg-[var(--color-error)]/10 px-4 py-3 text-sm text-[var(--color-error)]">
        {{ jobsError }}
      </div>

      <div class="overflow-x-auto">
        <table class="w-full text-left text-sm divide-y divide-[var(--border-app)]">
          <thead class="bg-[var(--bg-cell-empty)] text-xs uppercase text-[var(--text-secondary)] font-mono">
            <tr>
              <th class="px-4 py-3">Status</th>
              <th class="px-4 py-3">Topic</th>
              <th class="px-4 py-3">Grid</th>
              <th class="px-4 py-3">Game</th>
              <th class="px-4 py-3">Action</th>
              <th class="px-4 py-3">Created</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-[var(--border-app)] font-mono text-xs">
            <tr v-for="job in jobs" :key="job.id">
              <td class="px-4 py-3">
                <span :class="[
                  'px-2 py-0.5 rounded text-[10px] uppercase font-bold',
                  job.status === 'SUCCEEDED' ? 'bg-[var(--color-success)] text-slate-900' : 
                  job.status === 'FAILED' ? 'bg-[var(--color-error)] text-slate-900' :
                  'bg-[var(--color-warning)] text-slate-900'
                ]">{{ job.status }}</span>
              </td>
              <td class="px-4 py-3 text-[var(--text-primary)] font-sans text-sm font-medium">{{ job.topic }}</td>
              <td class="px-4 py-3 text-[var(--text-secondary)]">{{ job.width }}x{{ job.height }}</td>
              <td class="px-4 py-3">
                <div v-if="job.resultGame" class="flex flex-col gap-0.5">
                  <span class="text-[var(--text-primary)] font-sans text-sm font-medium">{{ job.resultGame.title }}</span>
                  <span class="text-[10px] uppercase font-bold text-[var(--text-secondary)]">
                    {{ job.resultGame.published ? "published" : "draft" }}
                  </span>
                </div>
                <span v-else class="text-[var(--text-secondary)]">-</span>
              </td>
              <td class="px-4 py-3">
                <button
                  v-if="job.resultGame && !job.resultGame.published"
                  class="app-btn text-xs"
                  :disabled="publishingGameId === job.resultGame.id"
                  @click="publishGame(job.resultGame.id)"
                >
                  Publish
                </button>
                <span v-else class="text-[var(--text-secondary)]">-</span>
              </td>
              <td class="px-4 py-3 text-[var(--text-secondary)]">{{ new Date(job.createdAt).toLocaleDateString() }}</td>
            </tr>
            <tr v-if="isLoadingJobs && !jobs.length">
              <td class="px-4 py-6 text-center text-[var(--text-secondary)]" colspan="6">Loading generation jobs...</td>
            </tr>
            <tr v-else-if="!jobs.length">
              <td class="px-4 py-6 text-center text-[var(--text-secondary)]" colspan="6">
                {{ jobsError ? "Unable to load generation jobs." : "No generation jobs found." }}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </main>
</template>

<script setup lang="ts">
definePageMeta({
  middleware: "auth",
});

const { data: user } = useAuth();
const { $client } = useNuxtApp();

const form = reactive({
  topic: "space exploration and planetary science",
  width: 21,
  height: 21,
  minWordLength: 3,
  maxWordLength: 12,
  targetWords: 42,
  runs: 20,
  maxAttempts: 180,
});

const jobs = ref<any[]>([]);
const generatedGame = ref<any | null>(null);
const errorMessage = ref("");
const isGenerating = ref(false);
const isLoadingJobs = ref(false);
const jobsError = ref("");
const publishingGameId = ref<string | null>(null);

// Live generation streaming state (fed by the runGeneration subscription).
const genLog = ref<any[]>([]);
const genProgress = ref<{ stage: string; current: number; total: number; message?: string } | null>(null);
const genStatus = ref<"idle" | "running" | "succeeded" | "failed">("idle");
const genStartedAt = ref<number | null>(null);
let genSub: { unsubscribe: () => void } | null = null;

async function refreshJobs() {
  if (!user.value?.user?.email) {
    jobsError.value = "Sign in again to load generation jobs.";
    return;
  }

  isLoadingJobs.value = true;
  jobsError.value = "";

  try {
    jobs.value = await $client.generator.listJobs.query({
      take: 25,
    });
  } catch (error) {
    jobsError.value = error instanceof Error ? error.message : String(error);
  } finally {
    isLoadingJobs.value = false;
  }
}

async function handleGenEvent(event: any) {
  if (event.type === "progress") {
    genProgress.value = {
      stage: event.stage,
      current: event.current,
      total: event.total,
      message: event.message,
    };
    return;
  }

  genLog.value = [...genLog.value, event];

  if (event.type === "completed") {
    genStatus.value = "succeeded";
    genProgress.value = null;
    try {
      const job = await $client.generator.getJob.query({
        id: event.jobId,
      });
      generatedGame.value = job?.resultGame ?? null;
    } catch {
      // The jobs table refresh below still surfaces the result.
    }
    await refreshJobs();
  } else if (event.type === "failed") {
    genStatus.value = "failed";
    errorMessage.value = event.error;
  }
}

function generate() {
  if (!user.value?.user?.email) return;

  // Reset for a fresh run and drop any previous subscription.
  genSub?.unsubscribe();
  errorMessage.value = "";
  generatedGame.value = null;
  genLog.value = [];
  genProgress.value = null;
  genStatus.value = "running";
  genStartedAt.value = Date.now();
  isGenerating.value = true;

  genSub = $client.generator.runGeneration.subscribe(
    {
      params: { ...form },
    },
    {
      onData(event: any) {
        void handleGenEvent(event);
      },
      onError(error: any) {
        errorMessage.value = error?.message ?? String(error);
        genStatus.value = "failed";
        genLog.value = [...genLog.value, { type: "failed", error: errorMessage.value, at: Date.now() }];
        isGenerating.value = false;
      },
      onComplete() {
        isGenerating.value = false;
        void refreshJobs();
      },
    }
  );
}

onBeforeUnmount(() => genSub?.unsubscribe());

async function publishGame(gameId: string) {
  if (!user.value?.user?.email) return;
  publishingGameId.value = gameId;
  errorMessage.value = "";

  try {
    const game = await $client.generator.publishGeneratedGame.mutate({
      gameId,
    });
    if (generatedGame.value?.id === game.id) {
      generatedGame.value = { ...generatedGame.value, published: true };
    }
    await refreshJobs();
  } catch (error) {
    errorMessage.value = error instanceof Error ? error.message : String(error);
  } finally {
    publishingGameId.value = null;
  }
}

watch(
  () => user.value?.user?.email,
  (email) => {
    if (email) void refreshJobs();
  },
  { immediate: true }
);
</script>

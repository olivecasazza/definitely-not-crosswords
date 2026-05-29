<script setup lang="ts">
definePageMeta({
  middleware: "auth",
});

const route = useRoute();
const { $client } = useNuxtApp();

const gameId = computed(() => route.params.id as string);
const isStarting = ref(false);
const startError = ref("");

const {
  data: game,
  pending,
  error,
} = await $client.activeGame.getStartDetails.useQuery({
  gameId: gameId.value,
});

const primaryActionLabel = computed(() => {
  if (game.value?.activeGameId) return "Continue Game";
  if (game.value?.completedGameId) return "Review Completed Game";
  return "Start Game";
});

async function startGame() {
  startError.value = "";

  if (game.value?.activeGameId) {
    await navigateTo(`/game/${game.value.activeGameId}`);
    return;
  }

  if (game.value?.completedGameId) {
    await navigateTo(`/game/${game.value.completedGameId}/completed`);
    return;
  }

  isStarting.value = true;
  try {
    const activeGame = await $client.activeGame.start.mutate({
      gameId: gameId.value,
    });
    await navigateTo(`/game/${activeGame.id}`);
  } catch (startFailure) {
    startError.value =
      startFailure instanceof Error ? startFailure.message : "Unable to start this game.";
  } finally {
    isStarting.value = false;
  }
}
</script>

<template>
  <div class="flex-grow w-full max-w-3xl mx-auto px-4 sm:px-6 py-8">
    <LoadingBar v-if="pending" />

    <div v-else-if="error || !game" class="app-card p-6 flex flex-col gap-4">
      <div>
        <h1 class="text-lg font-bold font-mono uppercase tracking-wider text-[var(--pastel-red)]">
          Game Unavailable
        </h1>
        <p class="mt-2 text-sm text-[var(--text-secondary)]">
          This puzzle could not be found or is not available to start.
        </p>
      </div>
      <NuxtLink to="/games" class="app-btn w-max">Back to Games</NuxtLink>
    </div>

    <div v-else class="flex flex-col gap-6">
      <div class="app-card p-6 sm:p-7 flex flex-col gap-6">
        <div class="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-4">
          <div class="min-w-0">
            <p class="text-xs font-mono uppercase tracking-widest text-[var(--text-secondary)]">
              New Game
            </p>
            <h1 class="mt-2 text-2xl sm:text-3xl font-bold font-serif text-[var(--text-primary)]">
              {{ game.title }}
            </h1>
          </div>

          <span
            class="w-max px-3 py-1.5 rounded border border-[var(--border-app)] bg-[var(--bg-cell-empty)] text-xs font-mono uppercase tracking-wider text-[var(--text-secondary)]"
          >
            {{ game.source.toLowerCase() }}
          </span>
        </div>

        <div class="grid grid-cols-1 sm:grid-cols-3 gap-3">
          <div class="border border-[var(--border-app)] bg-[var(--bg-cell-empty)] rounded-lg p-4">
            <p class="text-[10px] font-mono uppercase tracking-wider text-[var(--text-secondary)]">
              Clues
            </p>
            <p class="mt-1 text-xl font-bold text-[var(--text-primary)]">{{ game.questionCount }}</p>
          </div>

          <div class="border border-[var(--border-app)] bg-[var(--bg-cell-empty)] rounded-lg p-4">
            <p class="text-[10px] font-mono uppercase tracking-wider text-[var(--text-secondary)]">
              Grid
            </p>
            <p class="mt-1 text-xl font-bold text-[var(--text-primary)]">
              {{ game.gridSize }} x {{ game.gridSize }}
            </p>
          </div>

          <div class="border border-[var(--border-app)] bg-[var(--bg-cell-empty)] rounded-lg p-4">
            <p class="text-[10px] font-mono uppercase tracking-wider text-[var(--text-secondary)]">
              Status
            </p>
            <p class="mt-1 text-xl font-bold text-[var(--text-primary)]">
              {{ game.activeGameId ? "Active" : game.completedGameId ? "Completed" : "Ready" }}
            </p>
          </div>
        </div>

        <p v-if="startError" class="text-sm text-[var(--pastel-red)]">{{ startError }}</p>

        <div class="flex flex-col sm:flex-row gap-3">
          <button
            type="button"
            class="app-btn app-btn-active justify-center"
            :disabled="isStarting"
            @click="startGame"
          >
            {{ isStarting ? "Starting..." : primaryActionLabel }}
          </button>
          <NuxtLink to="/games" class="app-btn justify-center">Back to Games</NuxtLink>
        </div>
      </div>
    </div>
  </div>
</template>

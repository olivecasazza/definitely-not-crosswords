<script setup lang="ts">
const { data: user } = useAuth();
const { $client } = useNuxtApp()

const games = ref<unknown[]>([]);
const gamesError = ref("");

async function loadGames(email: string | null | undefined) {
  gamesError.value = "";

  if (!email) {
    games.value = [];
    return;
  }

  try {
    games.value = await $client.gameList.get.query({ email });
  } catch (error) {
    games.value = [];
    gamesError.value = error instanceof Error ? error.message : "Unable to load games.";
  }
}

watch(
  () => user.value?.user?.email,
  (email) => {
    void loadGames(email);
  },
  { immediate: true }
);
</script>

<template>
  <div class="p-6 flex flex-col items-center justify-center flex-grow">
    <div class="app-card p-6 w-full max-w-xl flex flex-col gap-4 font-mono text-xs">
      <h1 class="text-sm font-bold uppercase tracking-wider">New Game Details</h1>
      <pre class="bg-[var(--bg-cell-empty)] p-3 rounded border border-[var(--border-app)] overflow-x-auto text-[var(--text-secondary)]">{{ user }}</pre>
      <p v-if="gamesError" class="text-red-500">{{ gamesError }}</p>
      <pre v-if="user?.user?.email" class="bg-[var(--bg-cell-empty)] p-3 rounded border border-[var(--border-app)] overflow-x-auto text-[var(--text-secondary)]">{{ games }}</pre>
    </div>
  </div>
</template>

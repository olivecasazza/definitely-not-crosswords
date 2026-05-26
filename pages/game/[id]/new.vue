<script setup lang="ts">
const { data: user } = useAuth();
const { $client } = useNuxtApp()

const { data: activeGames } = await $client.activeGames.useQuery({ email: user.value?.user?.email });
</script>

<template>
  <div class="p-6 flex flex-col items-center justify-center flex-grow">
    <div class="app-card p-6 w-full max-w-xl flex flex-col gap-4 font-mono text-xs">
      <h1 class="text-sm font-bold uppercase tracking-wider">New Game Details</h1>
      <pre class="bg-[var(--bg-cell-empty)] p-3 rounded border border-[var(--border-app)] overflow-x-auto text-[var(--text-secondary)]">{{ user }}</pre>
      <pre v-if="user?.user?.email" class="bg-[var(--bg-cell-empty)] p-3 rounded border border-[var(--border-app)] overflow-x-auto text-[var(--text-secondary)]">{{ activeGames }}</pre>
    </div>
  </div>
</template>

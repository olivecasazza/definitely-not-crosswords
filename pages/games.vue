<template>
  <div class="flex-grow p-6 flex flex-col max-w-xl mx-auto w-full">
    <div class="app-card overflow-hidden">
      <div class="p-4 border-b border-[var(--border-app)]">
        <h1 class="text-lg font-bold font-mono tracking-wider">AVAILABLE GAMES</h1>
      </div>
      <LoadingBar v-if="pending" />
      <div v-else class="divide-y divide-[var(--border-app)]">
        <div v-for="game in games" :key="game.id" 
          class="flex flex-row items-center justify-between p-4 hover:bg-[var(--bg-cell-empty)] transition-all duration-150 cursor-pointer"
          @click="handleGameClick(game)">
          
          <div class="flex flex-col gap-0.5">
            <span class="font-bold text-sm text-[var(--text-primary)]">
              {{ game.type === 'Game' ? game.title : game.game.title }}
            </span>
          </div>

          <div>
            <span v-if="game.type === 'Game'" class="px-2.5 py-1 text-xs font-mono font-bold rounded uppercase border border-[var(--border-app)] text-[var(--text-secondary)]">
              unstarted
            </span>
            <span v-else-if="game.type === 'ActiveGame'" class="px-2.5 py-1 text-xs font-mono font-bold rounded uppercase bg-[var(--pastel-yellow)] text-slate-900">
              active
            </span>
            <span v-else-if="game.type === 'CompletedGame'" class="px-2.5 py-1 text-xs font-mono font-bold rounded uppercase bg-[var(--pastel-green)] text-slate-900">
              completed
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
definePageMeta({
  middleware: "auth",
});

const { data: user } = useAuth();
const { $client } = useNuxtApp();

const { data: games, pending } = await $client.gameList.get.useQuery({
  email: user.value?.user?.email as string,
});

function handleGameClick(game: any) {
  if (game.type === 'Game') {
    navigateTo(`/game/${game.id}/new`);
  } else if (game.type === 'ActiveGame') {
    navigateTo(`/game/${game.id}`);
  } else if (game.type === 'CompletedGame') {
    navigateTo(`/game/${game.id}/completed`);
  }
}
</script>

<template>
  <div class="flex-1 flex flex-col overflow-y-hidden p-6 gap-4 w-full max-w-4xl mx-auto">
    <main class="flex-1 overflow-y-auto flex flex-col gap-4">
      <LoadingBar v-if="activeGameLoading" />
      <div v-else class="flex flex-col gap-6">
        <GameBoard />
        <QuestionsList />
      </div>
    </main>
    <QuestionDirectionFilterBar />
  </div>
</template>

<script setup lang="ts">
import { storeToRefs } from 'pinia'
import { useActiveGameStore } from '~/stores/activeGame'

const activeGameStore = useActiveGameStore()
const { activeGameLoading } = storeToRefs(activeGameStore)

await activeGameStore.load();
</script>
<template>
  <div class="flex-grow w-full max-w-7xl mx-auto px-4 sm:px-6 py-6 overflow-y-auto">
    <LoadingBar v-if="activeGameLoading" />
    <div v-else class="grid grid-cols-1 lg:grid-cols-12 gap-6 items-start">
      <!-- Left Column: Game Board and the Active Clue Panel -->
      <div class="lg:col-span-7 flex flex-col gap-6 items-center w-full">
        <GameBoard />
        <ActiveClueCard />
      </div>

      <!-- Right Column: Clues List -->
      <div class="lg:col-span-5 w-full flex flex-col lg:max-h-[calc(100vh-120px)] overflow-hidden">
        <QuestionsList />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { storeToRefs } from 'pinia'
import { useActiveGameStore } from '~/stores/activeGame'

const activeGameStore = useActiveGameStore()
const { activeGameLoading } = storeToRefs(activeGameStore)

await activeGameStore.load();
</script>
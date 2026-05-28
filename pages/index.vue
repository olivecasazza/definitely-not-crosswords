<template>
  <div class="p-6 flex flex-col items-center justify-center flex-grow">
    <div class="app-card p-6 flex flex-col items-center gap-4 w-full max-w-sm">
      <img v-if="image" :src="image" alt="Profile" class="w-20 h-20 rounded-full border border-[var(--border-app)] shadow-sm">
      <div v-else class="w-20 h-20 rounded-full border border-[var(--border-app)] bg-[var(--bg-cell-empty)] flex items-center justify-center font-bold text-xl text-[var(--text-secondary)]">
        {{ name?.charAt(0) || 'U' }}
      </div>
      <div class="text-center">
        <h2 class="text-xl font-bold font-serif text-[var(--text-primary)]">{{ name }}</h2>
        <p class="text-sm font-sans text-[var(--text-secondary)]">{{ email }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { storeToRefs } from 'pinia'
import { useUserStore } from '~/stores/user'

definePageMeta({
  middleware: 'auth'
})

const userStore = useUserStore()
const { image, name, email } = storeToRefs(userStore)
</script>

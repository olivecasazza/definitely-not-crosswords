<template>
  <div class="flex flex-row gap-2 p-4 border-b border-[var(--border-app)] items-center">
    <button class="app-btn" @click="navigateTo('/')">home</button>
    <button class="app-btn" @click="navigateTo('/games')">games</button>
    <button v-if="isAdmin" class="app-btn" @click="navigateTo('/admin/generator')">generator</button>
    <div class="flex-grow"></div>
    
    <button class="app-btn text-xs font-mono uppercase" @click="isLight = !isLight">
      theme: {{ isLight ? 'light' : 'dark' }}
    </button>
    <button class="app-btn" @click="signOut()">sign out</button>
  </div>
</template>

<script setup lang="ts">
definePageMeta({
  middleware: "auth",
});
const { data, signOut } = useAuth();
const isAdmin = computed(() => {
  return (data.value?.user as { role?: string } | undefined)?.role === "ADMIN";
});

const isLight = useState('isLight');
</script>

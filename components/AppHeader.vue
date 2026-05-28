<template>
  <div class="flex flex-row items-center justify-between px-4 sm:px-6 py-3 border-b border-[var(--border-app)] bg-[rgba(24,24,27,0.78)] backdrop-blur-md transition-all duration-300 w-full select-none sticky top-0 z-50">
    
    <!-- Left Section: Logo & Branding -->
    <div class="flex items-center cursor-pointer select-none" @click="navigateTo('/')">
      <svg class="w-8 h-8 text-[var(--pastel-yellow)] shrink-0 hover:scale-105 active:scale-95 transition-all duration-200" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
        <!-- Row 0 -->
        <circle cx="2" cy="2" r="1.2" fill="currentColor" />
        <circle cx="6" cy="2" r="1.2" fill="currentColor" />
        <circle cx="10" cy="2" r="1.2" fill="currentColor" />
        <circle cx="14" cy="2" r="1.2" fill="currentColor" class="opacity-30" />
        <circle cx="18" cy="2" r="1.2" fill="currentColor" />
        <circle cx="22" cy="2" r="1.2" fill="currentColor" />

        <!-- Row 1 -->
        <circle cx="2" cy="6" r="1.2" fill="currentColor" />
        <circle cx="6" cy="6" r="1.2" fill="currentColor" />
        <circle cx="10" cy="6" r="1.2" fill="currentColor" />
        <circle cx="14" cy="6" r="1.2" fill="currentColor" class="opacity-30" />
        <circle cx="18" cy="6" r="1.2" fill="currentColor" />
        <circle cx="22" cy="6" r="1.2" fill="currentColor" />

        <!-- Row 2 -->
        <circle cx="2" cy="10" r="1.2" fill="currentColor" />
        <circle cx="6" cy="10" r="1.2" fill="currentColor" class="opacity-30" />
        <circle cx="10" cy="10" r="1.2" fill="currentColor" class="opacity-30" />
        <circle cx="14" cy="10" r="1.2" fill="currentColor" class="opacity-30" />
        <circle cx="18" cy="10" r="1.2" fill="currentColor" class="opacity-30" />
        <circle cx="22" cy="10" r="1.2" fill="currentColor" class="opacity-30" />

        <!-- Row 3 -->
        <circle cx="2" cy="14" r="1.2" fill="currentColor" />
        <circle cx="6" cy="14" r="1.2" fill="currentColor" />
        <circle cx="10" cy="14" r="1.2" fill="currentColor" />
        <circle cx="14" cy="14" r="1.2" fill="currentColor" class="opacity-30" />
        <circle cx="18" cy="14" r="1.2" fill="currentColor" />
        <circle cx="22" cy="14" r="1.2" fill="currentColor" />

        <!-- Row 4 -->
        <circle cx="2" cy="18" r="1.2" fill="currentColor" />
        <circle cx="6" cy="18" r="1.2" fill="currentColor" />
        <circle cx="10" cy="18" r="1.2" fill="currentColor" />
        <circle cx="14" cy="18" r="1.2" fill="currentColor" class="opacity-30" />
        <circle cx="18" cy="18" r="1.2" fill="currentColor" />
        <circle cx="22" cy="18" r="1.2" fill="currentColor" />

        <!-- Row 5 -->
        <circle cx="2" cy="22" r="1.2" fill="currentColor" />
        <circle cx="6" cy="22" r="1.2" fill="currentColor" />
        <circle cx="10" cy="22" r="1.2" fill="currentColor" />
        <circle cx="14" cy="22" r="1.2" fill="currentColor" />
        <circle cx="18" cy="22" r="1.2" fill="currentColor" />
        <circle cx="22" cy="22" r="1.2" fill="currentColor" />
      </svg>
    </div>

    <!-- Center Section: Clean Navigation Links -->
    <nav class="flex flex-row items-center gap-1 sm:gap-2">
      <button 
        v-for="link in navLinks" 
        :key="link.path"
        @click="navigateTo(link.path)"
        :class="[
          'px-2.5 sm:px-3 py-1.5 rounded-lg text-xs sm:text-sm font-medium tracking-wide transition-all duration-200 select-none cursor-pointer border',
          isCurrentRoute(link.path) 
            ? 'bg-[rgba(254,234,153,0.06)] text-[var(--pastel-yellow)] border-[rgba(254,234,153,0.15)] font-semibold'
            : 'bg-transparent text-[var(--text-secondary)] border-transparent hover:text-[var(--text-primary)] hover:bg-[rgba(255,255,255,0.02)]'
        ]"
      >
        {{ link.label }}
      </button>
    </nav>

    <!-- Right Section: Actions & Theme Toggle -->
    <div class="flex items-center gap-2.5">
      <!-- User Info Badge -->
      <div v-if="user" class="hidden md:flex items-center gap-2 border border-[var(--border-app)] bg-[rgba(24,24,27,0.4)] px-2.5 py-1 rounded-xl text-[10px] font-mono select-none">
        <img v-if="user.image" :src="user.image" class="w-4 h-4 rounded-full border border-[var(--border-app)]" />
        <div v-else class="w-4 h-4 rounded-full bg-[var(--border-app)] flex items-center justify-center font-bold text-[8px] text-[var(--text-secondary)] uppercase">
          {{ user.name?.charAt(0) || 'U' }}
        </div>
        <span class="text-[var(--text-secondary)] font-medium max-w-[80px] truncate">{{ user.name }}</span>
      </div>

      <!-- Theme Switch Button (Custom SVGs) -->
      <button 
        @click="isLight = !isLight" 
        class="w-8 h-8 rounded-lg border border-[var(--border-app)] bg-[var(--bg-card)] hover:bg-[var(--border-app)] hover:scale-105 active:scale-95 transition-all flex items-center justify-center cursor-pointer"
        :title="isLight ? 'Switch to Dark Theme' : 'Switch to Light Theme'"
      >
        <!-- Moon Icon -->
        <svg v-if="isLight" class="w-4 h-4 text-[var(--text-primary)]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
        </svg>
        <!-- Sun Icon -->
        <svg v-else class="w-4 h-4 text-[var(--pastel-yellow)]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364-6.364l-.707.707M6.343 17.657l-.707.707m0-12.728l.707.707m12.728 12.728l.707.707M12 8a4 4 0 100 8 4 4 0 000-8z" />
        </svg>
      </button>

      <!-- Sign Out Button -->
      <button 
        @click="signOut()" 
        class="px-2.5 py-1.5 text-[10px] font-mono uppercase tracking-wider rounded-lg border border-[var(--border-app)] bg-transparent hover:bg-[var(--border-hover)] hover:text-[var(--text-primary)] transition-all cursor-pointer font-semibold"
      >
        Sign Out
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useRoute } from 'vue-router';

definePageMeta({
  middleware: "auth",
});

const { data, signOut } = useAuth();
const user = computed(() => data.value?.user);

const isAdmin = computed(() => {
  return (data.value?.user as { role?: string } | undefined)?.role === "ADMIN";
});

const isLight = useState('isLight');

// Route path matching
const route = useRoute();
function isCurrentRoute(path: string): boolean {
  if (path === '/') return route.path === '/';
  return route.path.startsWith(path);
}

// Navigation links config
const navLinks = computed(() => {
  const links = [
    { label: 'Home', path: '/' },
    { label: 'Games', path: '/games' },
    { label: 'Stats', path: '/stats' }
  ];
  if (isAdmin.value) {
    links.push({ label: 'Generator', path: '/admin/generator' });
  }
  return links;
});
</script>

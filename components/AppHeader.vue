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

    <!-- Right Section: Actions & Profile Dropdown -->
    <div class="relative" ref="dropdownRef">
      <!-- Profile Button -->
      <button 
        @click="isDropdownOpen = !isDropdownOpen" 
        class="flex items-center gap-2 sm:gap-2.5 border border-[var(--border-app)] bg-[rgba(24,24,27,0.4)] hover:bg-[rgba(24,24,27,0.6)] px-2.5 py-1.5 sm:px-3 sm:py-2 rounded-xl text-[10px] sm:text-xs font-mono select-none cursor-pointer transition-all duration-200 outline-none hover:border-[var(--border-hover)] focus:border-[var(--border-hover)]"
        :aria-expanded="isDropdownOpen"
        aria-haspopup="true"
      >
        <img v-if="user?.image" :src="user.image" class="w-5 h-5 rounded-full border border-[var(--border-app)] shrink-0" />
        <div v-else class="w-5 h-5 rounded-full bg-[var(--border-app)] flex items-center justify-center font-bold text-[9px] text-[var(--text-secondary)] uppercase shrink-0">
          {{ user?.name?.charAt(0) || 'U' }}
        </div>
        <span class="hidden sm:inline text-[var(--text-secondary)] font-medium max-w-[90px] truncate">{{ user?.name }}</span>
        <!-- Small Down Arrow Icon -->
        <svg class="w-3 h-3 text-[var(--text-secondary)] transition-transform duration-200 shrink-0" :class="{ 'rotate-180': isDropdownOpen }" fill="none" viewBox="0 0 24 24" stroke="currentColor">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      <!-- Dropdown Modal -->
      <Transition
        enter-active-class="transition ease-out duration-150"
        enter-from-class="transform opacity-0 scale-95 -translate-y-2"
        enter-to-class="transform opacity-100 scale-100 translate-y-0"
        leave-active-class="transition ease-in duration-100"
        leave-from-class="transform opacity-100 scale-100 translate-y-0"
        leave-to-class="transform opacity-0 scale-95 -translate-y-2"
      >
        <div 
          v-if="isDropdownOpen" 
          class="absolute right-0 mt-2 w-56 border border-[var(--border-app)] bg-[var(--bg-card)] rounded-xl shadow-2xl z-50 select-none overflow-hidden backdrop-blur-lg"
        >
          <!-- User Identity Header -->
          <div class="px-4 py-3 border-b border-[var(--border-app)] flex flex-col gap-0.5 select-none bg-[rgba(0,0,0,0.05)]">
            <span class="text-xs font-bold text-[var(--text-primary)] truncate">{{ user?.name || 'User' }}</span>
            <span class="text-[9px] font-mono text-[var(--text-secondary)] uppercase tracking-wider font-semibold">{{ user?.role || 'Player' }}</span>
          </div>

          <!-- Navigation Links -->
          <div class="py-1">
            <button 
              v-for="link in regularLinks" 
              :key="link.path"
              @click="handleNavigate(link.path)"
              class="w-full text-left px-4 py-2.5 text-xs font-medium tracking-wide transition-colors flex items-center justify-between cursor-pointer"
              :class="isCurrentRoute(link.path)
                ? 'bg-[rgba(254,234,153,0.05)] text-[var(--pastel-yellow)] font-semibold'
                : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[rgba(255,255,255,0.02)]'"
            >
              <span>{{ link.label }}</span>
              <span v-if="isCurrentRoute(link.path)" class="w-1.5 h-1.5 rounded-full bg-[var(--pastel-yellow)]"></span>
            </button>
          </div>

          <!-- Admin Separator & Link -->
          <template v-if="isAdmin">
            <div class="border-t border-[var(--border-app)] my-0.5"></div>
            <div class="py-1">
              <button 
                @click="handleNavigate('/admin')"
                class="w-full text-left px-4 py-2.5 text-xs font-semibold tracking-wide transition-colors flex items-center justify-between cursor-pointer"
                :class="isCurrentRoute('/admin')
                  ? 'bg-[rgba(254,234,153,0.05)] text-[var(--pastel-yellow)]'
                  : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[rgba(255,255,255,0.02)]'"
              >
                <span>Admin Controls</span>
                <span v-if="isCurrentRoute('/admin')" class="w-1.5 h-1.5 rounded-full bg-[var(--pastel-yellow)]"></span>
              </button>
            </div>
          </template>

          <!-- Settings Separator -->
          <div class="border-t border-[var(--border-app)] my-0.5"></div>

          <!-- Settings & Actions -->
          <div class="py-1">
            <!-- Theme Switch Dropdown Item -->
            <button 
              @click="isLight = !isLight" 
              class="w-full flex items-center justify-between px-4 py-2.5 text-xs font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[rgba(255,255,255,0.02)] transition-colors select-none text-left cursor-pointer"
            >
              <span>Theme</span>
              <span class="flex items-center gap-1.5 text-[10px] font-semibold text-[var(--text-secondary)] uppercase tracking-wider font-mono">
                {{ isLight ? 'Light' : 'Dark' }}
                <!-- Moon Icon -->
                <svg v-if="isLight" class="w-3.5 h-3.5 text-[var(--text-primary)]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
                </svg>
                <!-- Sun Icon -->
                <svg v-else class="w-3.5 h-3.5 text-[var(--pastel-yellow)]" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364-6.364l-.707.707M6.343 17.657l-.707.707m0-12.728l.707.707m12.728 12.728l.707.707M12 8a4 4 0 100 8 4 4 0 000-8z" />
                </svg>
              </span>
            </button>

            <!-- Sign Out Dropdown Item -->
            <button 
              @click="handleSignOut"
              class="w-full text-left px-4 py-2.5 text-xs font-mono uppercase tracking-wider text-[var(--pastel-red)] hover:bg-[rgba(255,255,255,0.02)] transition-colors select-none cursor-pointer flex items-center gap-2 font-semibold"
            >
              <svg class="w-3.5 h-3.5 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
              </svg>
              Sign Out
            </button>
          </div>
        </div>
      </Transition>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { useRoute } from 'vue-router';
import { roleHasCapability } from '~/lib/auth/roles';

const { data, signOut } = useAuth();
const user = computed(() => data.value?.user);

const isAdmin = computed(() => {
  return roleHasCapability((data.value?.user as { role?: string } | undefined)?.role, "admin:access");
});

const isLight = useState('isLight');

// Dropdown State & Ref
const isDropdownOpen = ref(false);
const dropdownRef = ref<HTMLElement | null>(null);

// Route path matching
const route = useRoute();
function isCurrentRoute(path: string): boolean {
  if (path === '/') return route.path === '/';
  return route.path.startsWith(path);
}

// Navigation links configs
const regularLinks = [
  { label: 'Home', path: '/' },
  { label: 'Games', path: '/games' },
  { label: 'Stats', path: '/stats' }
];

function handleNavigate(path: string) {
  isDropdownOpen.value = false;
  navigateTo(path);
}

function handleSignOut() {
  isDropdownOpen.value = false;
  signOut();
}

// Click Outside Handler
function handleClickOutside(event: MouseEvent) {
  if (dropdownRef.value && !dropdownRef.value.contains(event.target as Node)) {
    isDropdownOpen.value = false;
  }
}

// Escape Key Handler
function handleKeyDown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    isDropdownOpen.value = false;
  }
}

onMounted(() => {
  document.addEventListener('click', handleClickOutside);
  document.addEventListener('keydown', handleKeyDown);
});

onUnmounted(() => {
  document.removeEventListener('click', handleClickOutside);
  document.removeEventListener('keydown', handleKeyDown);
});
</script>

<template>
  <div class="min-h-screen flex flex-col bg-[var(--bg-app)] text-[var(--text-primary)] font-sans">
    <!-- Sticky Header Wrapper -->
    <header class="sticky top-0 z-50">
      <AppHeader />
    </header>

    <!-- Main View Content Slot -->
    <main class="flex-grow flex flex-col">
      <slot />
    </main>

    <!-- Premium Responsive Footer -->
    <footer class="mt-auto border-t border-[var(--border-app)] bg-[var(--bg-card)] py-6 text-center text-xs text-[var(--text-secondary)] select-none">
      <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 flex flex-col sm:flex-row justify-between items-center gap-4">
        <div class="flex items-center gap-3">
          <svg class="w-5 h-5 text-[var(--pastel-yellow)] shrink-0" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
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
          <span class="font-bold text-[var(--text-primary)] tracking-wide font-mono uppercase text-sm">Definitely Not Crosswords</span>
        </div>
        <div class="font-medium text-[var(--text-secondary)] opacity-80">
          &copy; {{ new Date().getFullYear() }} Definitely Not Crosswords. All rights reserved.
        </div>
        <div class="flex gap-5 font-mono text-[10px] uppercase tracking-widest">
          <a href="https://github.com/olivecasazza" target="_blank" class="hover:text-[var(--text-primary)] transition-colors">GitHub</a>
          <a href="#" class="hover:text-[var(--text-primary)] transition-colors">Privacy</a>
          <a href="#" class="hover:text-[var(--text-primary)] transition-colors">Terms</a>
        </div>
      </div>
    </footer>
  </div>
</template>
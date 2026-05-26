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
        <div class="flex items-center gap-2">
          <span class="text-base">🧩</span>
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
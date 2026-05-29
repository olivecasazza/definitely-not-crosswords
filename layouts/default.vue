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
    <AppFooter />
  </div>
</template>
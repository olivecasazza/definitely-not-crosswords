<template>
  <main class="flex-grow p-6 w-full max-w-5xl mx-auto flex flex-col gap-6">
    <div class="app-card p-6 flex flex-col gap-6">
      <AdminNav />

      <div class="grid gap-4 md:grid-cols-2">
        <button
          class="app-card p-5 text-left transition hover:border-[var(--border-hover)]"
          type="button"
          @click="navigateTo('/admin/generator')"
        >
          <div class="text-sm font-bold uppercase tracking-wider text-[var(--text-primary)]">Generator</div>
          <div class="mt-2 text-xs leading-relaxed text-[var(--text-secondary)]">
            Create, review, and publish generated crossword drafts.
          </div>
        </button>
        <button
          class="app-card p-5 text-left transition hover:border-[var(--border-hover)]"
          type="button"
          @click="navigateTo('/admin/users')"
        >
          <div class="text-sm font-bold uppercase tracking-wider text-[var(--text-primary)]">Users</div>
          <div class="mt-2 text-xs leading-relaxed text-[var(--text-secondary)]">
            Add admins and manage user roles.
          </div>
        </button>
      </div>
    </div>
  </main>
</template>

<script setup lang="ts">
definePageMeta({
  middleware: "auth",
});
</script>

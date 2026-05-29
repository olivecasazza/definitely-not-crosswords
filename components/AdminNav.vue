<template>
  <div class="flex flex-col gap-3 border-b border-[var(--border-app)] pb-4">
    <div class="flex flex-col gap-1 sm:flex-row sm:items-end sm:justify-between">
      <div>
        <h1 class="text-lg font-bold font-primary tracking-wider">ADMIN</h1>
        <p class="text-xs text-[var(--text-secondary)]">Operational controls for puzzles, users, and roles.</p>
      </div>
    </div>
    <nav class="flex flex-wrap gap-2">
      <button
        v-for="item in items"
        :key="item.path"
        type="button"
        :class="[
          'app-btn text-xs font-mono uppercase tracking-wider',
          isCurrentRoute(item.path) ? 'app-btn-active' : ''
        ]"
        @click="navigateTo(item.path)"
      >
        {{ item.label }}
      </button>
    </nav>
  </div>
</template>

<script setup lang="ts">
const route = useRoute();

const items = [
  { label: "Overview", path: "/admin" },
  { label: "Generator", path: "/admin/generator" },
  { label: "Users", path: "/admin/users" },
];

function isCurrentRoute(path: string) {
  if (path === "/admin") {
    return route.path === "/admin";
  }
  return route.path.startsWith(path);
}
</script>

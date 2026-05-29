<template>
  <main class="flex-grow p-6 w-full max-w-6xl mx-auto flex flex-col gap-6">
    <div class="app-card p-6 flex flex-col gap-6">
      <AdminNav />

      <form class="grid gap-3 md:grid-cols-[1fr_1fr_160px_auto] md:items-end" @submit.prevent="addUser">
        <label class="flex flex-col gap-1 text-xs uppercase tracking-wider text-[var(--text-secondary)]">
          Email
          <input v-model="newUser.email" class="app-input px-3 py-2 text-sm normal-case" type="email" required />
        </label>
        <label class="flex flex-col gap-1 text-xs uppercase tracking-wider text-[var(--text-secondary)]">
          Name
          <input v-model="newUser.name" class="app-input px-3 py-2 text-sm normal-case" type="text" />
        </label>
        <label class="flex flex-col gap-1 text-xs uppercase tracking-wider text-[var(--text-secondary)]">
          Role
          <select v-model="newUser.role" class="app-input px-3 py-2 text-sm">
            <option v-for="option in roleOptions" :key="option.role" :value="option.role">
              {{ option.role }}
            </option>
          </select>
        </label>
        <button class="app-btn app-btn-active h-[38px] font-bold" type="submit" :disabled="saving">
          {{ saving ? "Saving..." : "Add User" }}
        </button>
      </form>

      <div v-if="message" class="rounded-md border border-[var(--color-success)] bg-[var(--color-success)]/10 p-3 text-sm text-[var(--color-success)]">
        {{ message }}
      </div>
      <div v-if="errorMessage" class="rounded-md border border-[var(--color-error)] bg-[var(--color-error)]/10 p-3 text-sm text-[var(--color-error)]">
        {{ errorMessage }}
      </div>

      <div class="overflow-x-auto">
        <table class="w-full text-left text-sm divide-y divide-[var(--border-app)]">
          <thead class="bg-[var(--bg-cell-empty)] text-xs uppercase text-[var(--text-secondary)] font-mono">
            <tr>
              <th class="px-4 py-3">User</th>
              <th class="px-4 py-3">Username</th>
              <th class="px-4 py-3">Verified</th>
              <th class="px-4 py-3">Role</th>
              <th class="px-4 py-3">Capabilities</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-[var(--border-app)]">
            <tr v-for="user in users" :key="user.id">
              <td class="px-4 py-3">
                <div class="font-medium text-[var(--text-primary)]">{{ user.name || user.email || "Unnamed user" }}</div>
                <div class="text-xs text-[var(--text-secondary)]">{{ user.email || "-" }}</div>
              </td>
              <td class="px-4 py-3 text-[var(--text-secondary)]">{{ user.username || "-" }}</td>
              <td class="px-4 py-3">
                <span :class="[
                  'rounded px-2 py-0.5 text-[10px] font-bold uppercase',
                  user.emailVerified ? 'bg-[var(--color-success)] text-slate-900' : 'bg-[var(--border-app)] text-[var(--text-secondary)]'
                ]">
                  {{ user.emailVerified ? "Verified" : "Pending" }}
                </span>
              </td>
              <td class="px-4 py-3">
                <select
                  class="app-input px-2 py-1.5 text-xs"
                  :value="user.role"
                  :disabled="savingRoleId === user.id"
                  @change="setRole(user.id, ($event.target as HTMLSelectElement).value)"
                >
                  <option v-for="option in roleOptions" :key="option.role" :value="option.role">
                    {{ option.role }}
                  </option>
                </select>
              </td>
              <td class="px-4 py-3">
                <div class="flex flex-wrap gap-1">
                  <span
                    v-for="capability in capabilitiesForRole(user.role)"
                    :key="capability"
                    class="rounded border border-[var(--border-app)] px-2 py-0.5 text-[10px] text-[var(--text-secondary)]"
                  >
                    {{ capability }}
                  </span>
                </div>
              </td>
            </tr>
            <tr v-if="!users.length && !loading">
              <td class="px-4 py-6 text-center text-[var(--text-secondary)]" colspan="5">No users found.</td>
            </tr>
            <tr v-if="loading">
              <td class="px-4 py-6 text-center text-[var(--text-secondary)]" colspan="5">Loading users...</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </main>
</template>

<script setup lang="ts">
definePageMeta({
  middleware: "auth",
});

type AdminUser = {
  id: string;
  email: string | null;
  username: string | null;
  name: string | null;
  role: string;
  emailVerified: string | Date | null;
};

type RoleOption = {
  role: string;
  capabilities: string[];
};

const { $client } = useNuxtApp();

const users = ref<AdminUser[]>([]);
const roleOptions = ref<RoleOption[]>([]);
const loading = ref(true);
const saving = ref(false);
const savingRoleId = ref<string | null>(null);
const message = ref("");
const errorMessage = ref("");

const newUser = reactive({
  email: "",
  name: "",
  role: "ADMIN",
});

function capabilitiesForRole(role: string) {
  return roleOptions.value.find((option) => option.role === role)?.capabilities ?? [];
}

async function refreshUsers() {
  users.value = await $client.user.listForAdmin.query();
}

async function load() {
  loading.value = true;
  errorMessage.value = "";
  try {
    const [roles] = await Promise.all([
      $client.user.roleOptions.query(),
      refreshUsers(),
    ]);
    roleOptions.value = roles;
    if (!roleOptions.value.some((option) => option.role === newUser.role)) {
      newUser.role = roleOptions.value[0]?.role ?? "ADMIN";
    }
  } catch (error: any) {
    errorMessage.value = error.message || "Unable to load admin users.";
  } finally {
    loading.value = false;
  }
}

async function addUser() {
  saving.value = true;
  message.value = "";
  errorMessage.value = "";
  try {
    await $client.user.upsertFromAdmin.mutate({
      email: newUser.email,
      name: newUser.name || undefined,
      role: newUser.role as any,
    });
    message.value = `${newUser.email} is now ${newUser.role}.`;
    newUser.email = "";
    newUser.name = "";
    newUser.role = "ADMIN";
    await refreshUsers();
  } catch (error: any) {
    errorMessage.value = error.message || "Unable to add user.";
  } finally {
    saving.value = false;
  }
}

async function setRole(userId: string, role: string) {
  savingRoleId.value = userId;
  message.value = "";
  errorMessage.value = "";
  try {
    await $client.user.setRole.mutate({ userId, role: role as any });
    message.value = "Role updated.";
    await refreshUsers();
  } catch (error: any) {
    errorMessage.value = error.message || "Unable to update role.";
    await refreshUsers();
  } finally {
    savingRoleId.value = null;
  }
}

onMounted(load);
</script>

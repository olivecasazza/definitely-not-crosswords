<template>
  <div class="min-h-screen py-10 px-4 sm:px-6 lg:px-8 bg-gradient-to-br from-[var(--bg-app)] via-[var(--bg-card)] to-[var(--bg-app)] relative overflow-hidden select-none">
    
    <!-- Background glow/blobs -->
    <div class="absolute top-1/4 left-1/4 w-96 h-96 bg-[var(--pastel-yellow)] opacity-10 rounded-full blur-[120px] pointer-events-none"></div>
    <div class="absolute bottom-1/4 right-1/4 w-96 h-96 bg-[var(--pastel-green)] opacity-10 rounded-full blur-[120px] pointer-events-none"></div>

    <div class="max-w-3xl mx-auto space-y-8 relative z-10">
      
      <!-- Back Button -->
      <div>
        <NuxtLink 
          to="/" 
          class="inline-flex items-center gap-2 px-3 py-1.5 rounded-lg border border-[var(--border-app)] bg-[rgba(24,24,27,0.4)] hover:bg-[var(--border-hover)] text-xs font-mono uppercase tracking-wider text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-all cursor-pointer"
        >
          <svg class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M10 19l-7-7m0 0l7-7m-7 7h18" />
          </svg>
          Back to Lobby
        </NuxtLink>
      </div>

      <!-- Main Profile Layout -->
      <div class="grid grid-cols-1 md:grid-cols-3 gap-8">
        
        <!-- Left Column: Avatar & Quick Info -->
        <div class="md:col-span-1 p-6 rounded-2xl border border-[var(--border-app)] bg-[rgba(24,24,27,0.6)] backdrop-blur-md flex flex-col items-center text-center space-y-4">
          <div class="relative group">
            <div class="w-24 h-24 rounded-full bg-gradient-to-tr from-[var(--pastel-yellow)] to-[rgba(254,234,153,0.3)] flex items-center justify-center font-bold text-3xl text-slate-900 border-4 border-[rgba(255,255,255,0.05)] shadow-xl relative select-none uppercase">
              {{ userName?.charAt(0) || 'U' }}
            </div>
            <!-- Verification badge -->
            <div 
              class="absolute bottom-0 right-0 w-6 h-6 rounded-full bg-[var(--pastel-green)] text-slate-900 border-2 border-[var(--bg-card)] flex items-center justify-center shadow-md"
              title="Email Verified"
            >
              <svg class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
              </svg>
            </div>
          </div>

          <div class="space-y-1">
            <h2 id="profile-display-name" class="font-bold text-lg text-[var(--text-primary)]">{{ userName }}</h2>
            <p class="text-xs text-[var(--text-secondary)] font-mono">{{ userEmail }}</p>
          </div>

          <div class="pt-4 border-t border-[var(--border-app)] w-full text-left space-y-2">
            <div class="flex justify-between text-[10px] font-mono uppercase tracking-wider text-[var(--text-secondary)]">
              <span>Account Type:</span>
              <span class="text-[var(--pastel-yellow)] font-semibold">{{ userRole }}</span>
            </div>
            <div class="flex justify-between text-[10px] font-mono uppercase tracking-wider text-[var(--text-secondary)]">
              <span>Status:</span>
              <span class="text-[var(--pastel-green)] font-semibold">Verified</span>
            </div>
          </div>
        </div>

        <!-- Right Column: Settings Form -->
        <div class="md:col-span-2 space-y-6">
          
          <!-- Update Profile Details Card -->
          <div class="p-6 sm:p-8 rounded-2xl border border-[var(--border-app)] bg-[rgba(24,24,27,0.6)] backdrop-blur-md space-y-6">
            <div>
              <h3 class="text-lg font-bold font-mono uppercase tracking-wider text-[var(--text-primary)]">Profile Settings</h3>
              <p class="text-xs text-[var(--text-secondary)] font-mono">Update your public identity details</p>
            </div>

            <!-- Alerts -->
            <div v-if="successMsg" id="profile-success-alert" class="p-3.5 rounded-xl border border-[rgba(168,230,207,0.2)] bg-[rgba(168,230,207,0.06)] text-[var(--pastel-green)] text-xs font-mono">
              {{ successMsg }}
            </div>
            <div v-if="errorMsg" class="p-3.5 rounded-xl border border-[rgba(255,140,140,0.2)] bg-[rgba(255,140,140,0.06)] text-[var(--pastel-red)] text-xs font-mono">
              {{ errorMsg }}
            </div>

            <form @submit.prevent="updateProfileName" class="space-y-4">
              <div class="space-y-1.5">
                <label for="profile-name-input" class="block text-xs font-semibold uppercase tracking-wider text-[var(--text-secondary)] font-mono">Display Name</label>
                <input 
                  id="profile-name-input"
                  v-model="form.name"
                  type="text" 
                  required
                  placeholder="e.g. Olive Casazza" 
                  class="w-full px-4 py-3 rounded-xl border border-[var(--border-app)] bg-[rgba(18,18,18,0.5)] focus:border-[var(--pastel-yellow)] focus:ring-1 focus:ring-[var(--pastel-yellow)] outline-none text-sm transition-all duration-300"
                />
              </div>

              <button 
                type="submit" 
                :disabled="updating"
                class="w-full py-3 px-4 rounded-xl font-semibold text-sm tracking-wider uppercase bg-gradient-to-r from-[var(--pastel-yellow)] to-[rgba(254,234,153,0.7)] text-slate-900 shadow-md hover:scale-[1.02] active:scale-[0.98] disabled:opacity-50 transition-all duration-300 cursor-pointer flex items-center justify-center gap-2"
              >
                <span v-if="updating" class="animate-spin inline-block w-4 h-4 border-2 border-slate-900 border-t-transparent rounded-full"></span>
                <span>{{ updating ? "Saving..." : "Update Profile" }}</span>
              </button>
            </form>
          </div>

          <!-- Danger Zone Card -->
          <div class="p-6 sm:p-8 rounded-2xl border border-[rgba(255,140,140,0.15)] bg-[rgba(255,140,140,0.03)] space-y-6">
            <div>
              <h3 class="text-lg font-bold font-mono uppercase tracking-wider text-[var(--pastel-red)]">Danger Zone</h3>
              <p class="text-xs text-[var(--text-secondary)] font-mono">Permanently remove your account and all associated data</p>
            </div>

            <div v-if="!showDeleteConfirm" class="pt-2">
              <button 
                id="delete-account-btn"
                @click="showDeleteConfirm = true"
                class="px-4 py-3 rounded-xl font-semibold text-xs tracking-wider uppercase border border-[var(--pastel-red)] text-[var(--pastel-red)] hover:bg-[rgba(255,140,140,0.06)] hover:scale-[1.02] active:scale-[0.98] transition-all duration-300 cursor-pointer"
              >
                Delete Account
              </button>
            </div>

            <!-- Delete Confirmation UI -->
            <div v-else class="space-y-4 p-4 rounded-xl border border-[rgba(255,140,140,0.2)] bg-[rgba(255,140,140,0.05)]">
              <h4 class="text-sm font-bold text-[var(--pastel-red)] uppercase font-mono">Are you absolutely sure?</h4>
              <p class="text-xs text-[var(--text-secondary)] leading-relaxed">
                This action is irreversible. All of your custom stats, generation jobs, and account references will be deleted forever.
              </p>
              
              <div class="flex flex-wrap gap-3 pt-2">
                <button 
                  id="confirm-delete-btn"
                  @click="deleteAccount"
                  :disabled="deleting"
                  class="px-4 py-2.5 rounded-lg font-semibold text-xs uppercase bg-[var(--pastel-red)] text-slate-900 shadow-md hover:scale-[1.02] active:scale-[0.98] disabled:opacity-50 transition-all cursor-pointer"
                >
                  {{ deleting ? "Deleting..." : "Yes, Delete Account" }}
                </button>
                <button 
                  id="cancel-delete-btn"
                  @click="showDeleteConfirm = false"
                  :disabled="deleting"
                  class="px-4 py-2.5 rounded-lg font-semibold text-xs uppercase border border-[var(--border-app)] text-[var(--text-primary)] hover:bg-[rgba(255,255,255,0.02)] transition-all cursor-pointer"
                >
                  Cancel
                </button>
              </div>
            </div>
          </div>

        </div>

      </div>

    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, onMounted } from "vue";

definePageMeta({
  middleware: "auth",
});

const { data, getSession, signOut } = useAuth();
const { $client } = useNuxtApp();

const userEmail = computed(() => data.value?.user?.email || "");
const userName = ref(data.value?.user?.name || "");
const userRole = computed(() => (data.value?.user as any)?.role || "USER");

const form = reactive({
  name: userName.value,
});

const updating = ref(false);
const successMsg = ref("");
const errorMsg = ref("");

const showDeleteConfirm = ref(false);
const deleting = ref(false);

onMounted(() => {
  if (data.value?.user?.name) {
    userName.value = data.value.user.name;
    form.name = data.value.user.name;
  }
  if (typeof window !== "undefined") {
    (window as any).__nuxt_hydrated = true;
  }
});

async function updateProfileName() {
  updating.value = true;
  successMsg.value = "";
  errorMsg.value = "";

  try {
    const res = await $client.user.updateProfile.mutate({
      email: userEmail.value,
      name: form.name,
    });

    if (res.success) {
      userName.value = res.name;
      form.name = res.name;
      successMsg.value = "Profile updated successfully!";
      
      // Attempt session refresh if supported
      try {
        await getSession();
      } catch (e) {
        // ignore session refresh errors
      }
    }
  } catch (err: any) {
    errorMsg.value = err.message || "Failed to update profile name.";
  } finally {
    updating.value = false;
  }
}

async function deleteAccount() {
  deleting.value = true;
  errorMsg.value = "";

  try {
    const res = await $client.user.deleteAccount.mutate({
      email: userEmail.value,
    });

    if (res.success) {
      await signOut({ callbackUrl: "/auth/signup" });
    }
  } catch (err: any) {
    errorMsg.value = err.message || "Failed to delete account.";
    deleting.value = false;
    showDeleteConfirm.value = false;
  }
}
</script>

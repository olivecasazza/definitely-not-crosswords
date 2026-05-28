<template>
  <div class="min-h-screen flex flex-col justify-center items-center px-4 bg-gradient-to-br from-[var(--bg-app)] via-[var(--bg-card)] to-[var(--bg-app)] relative overflow-hidden select-none">
    
    <!-- Background glow/blobs -->
    <div class="absolute top-1/4 left-1/4 w-96 h-96 bg-[var(--pastel-yellow)] opacity-10 rounded-full blur-[120px] pointer-events-none"></div>
    <div class="absolute bottom-1/4 right-1/4 w-96 h-96 bg-[var(--pastel-green)] opacity-10 rounded-full blur-[120px] pointer-events-none"></div>

    <div class="w-full max-w-md p-8 rounded-2xl border border-[var(--border-app)] bg-[rgba(24,24,27,0.7)] backdrop-blur-xl shadow-2xl relative z-10">
      
      <!-- Brand Logo & Header -->
      <div class="flex flex-col items-center mb-8">
        <div class="w-12 h-12 rounded-2xl bg-gradient-to-tr from-[var(--pastel-yellow)] to-[rgba(254,234,153,0.3)] flex items-center justify-center shadow-lg transform rotate-6 mb-4">
          <svg class="w-6 h-6 text-slate-900" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M18 9v3m0 0v3m0-3h3m-3 0h-3m-2-5a4 4 0 11-8 0 4 4 0 018 0zM3 20a6 6 0 0112 0v1H3v-1z" />
          </svg>
        </div>
        <h1 class="text-2xl font-bold font-mono tracking-wider text-[var(--text-primary)] uppercase">Create Account</h1>
        <p class="text-xs text-[var(--text-secondary)] mt-1 text-center font-mono">Join the "Definitely Not Crosswords" experience</p>
      </div>

      <form @submit.prevent="handleSignup" class="space-y-5">
        <!-- Error & Success alerts -->
        <div v-if="error" class="p-3.5 rounded-xl border border-[rgba(255,140,140,0.2)] bg-[rgba(255,140,140,0.06)] text-[var(--pastel-red)] text-xs font-mono">
          {{ error }}
        </div>

        <div v-if="success" class="p-3.5 rounded-xl border border-[rgba(168,230,207,0.2)] bg-[rgba(168,230,207,0.06)] text-[var(--pastel-green)] text-xs font-mono space-y-3">
          <p>Registration successful! Please verify your email.</p>
          <div v-if="verificationToken" class="pt-2 border-t border-[rgba(168,230,207,0.15)]">
            <p class="text-[10px] text-[var(--text-secondary)] mb-1">Testing verification link:</p>
            <NuxtLink 
              id="verification-link" 
              :to="'/auth/verify-email?token=' + verificationToken"
              class="underline font-semibold hover:text-[var(--text-primary)] transition-all text-[var(--pastel-yellow)]"
            >
              Verify Email ({{ verificationToken.substring(0, 8) }}...)
            </NuxtLink>
          </div>
        </div>

        <!-- Name Input -->
        <div class="space-y-1.5">
          <label for="name" class="block text-xs font-semibold uppercase tracking-wider text-[var(--text-secondary)] font-mono">Full Name</label>
          <div class="relative">
            <input 
              id="name"
              v-model="form.name"
              type="text" 
              required
              placeholder="e.g. Olive Casazza" 
              class="w-full px-4 py-3 rounded-xl border border-[var(--border-app)] bg-[rgba(18,18,18,0.5)] focus:border-[var(--pastel-yellow)] focus:ring-1 focus:ring-[var(--pastel-yellow)] outline-none text-sm transition-all duration-300"
            />
          </div>
        </div>

        <!-- Email Input -->
        <div class="space-y-1.5">
          <label for="email" class="block text-xs font-semibold uppercase tracking-wider text-[var(--text-secondary)] font-mono">Email Address</label>
          <div class="relative">
            <input 
              id="email"
              v-model="form.email"
              type="email" 
              required
              placeholder="e.g. olive.casazza@gmail.com" 
              class="w-full px-4 py-3 rounded-xl border border-[var(--border-app)] bg-[rgba(18,18,18,0.5)] focus:border-[var(--pastel-yellow)] focus:ring-1 focus:ring-[var(--pastel-yellow)] outline-none text-sm transition-all duration-300"
            />
          </div>
        </div>

        <!-- Submit Button -->
        <button 
          type="submit" 
          :disabled="loading"
          class="w-full py-3 px-4 rounded-xl font-semibold text-sm tracking-wider uppercase bg-gradient-to-r from-[var(--pastel-yellow)] to-[rgba(254,234,153,0.7)] text-slate-900 shadow-md hover:scale-[1.02] active:scale-[0.98] disabled:opacity-50 transition-all duration-300 cursor-pointer flex items-center justify-center gap-2"
        >
          <span v-if="loading" class="animate-spin inline-block w-4 h-4 border-2 border-slate-900 border-t-transparent rounded-full"></span>
          <span>{{ loading ? "Creating..." : "Sign Up" }}</span>
        </button>
      </form>

      <!-- Footer navigation -->
      <div class="mt-6 pt-6 border-t border-[var(--border-app)] text-center">
        <p class="text-xs text-[var(--text-secondary)] font-mono">
          Already have an account? 
          <NuxtLink to="/api/auth/signin" class="text-[var(--pastel-yellow)] hover:underline">Sign In</NuxtLink>
        </p>
      </div>

    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive } from "vue";

definePageMeta({
  auth: false,
});

const { $client } = useNuxtApp();

const form = reactive({
  name: "",
  email: "",
});

const loading = ref(false);
const error = ref("");
const success = ref(false);
const verificationToken = ref("");

async function handleSignup() {
  loading.value = true;
  error.value = "";
  success.value = false;
  
  try {
    const res = await $client.user.signup.mutate({
      name: form.name,
      email: form.email,
    });
    
    if (res.success) {
      success.value = true;
      verificationToken.value = res.verificationToken;
      form.name = "";
      form.email = "";
    }
  } catch (err: any) {
    error.value = err.message || "An error occurred during signup.";
  } finally {
    loading.value = false;
  }
}
</script>

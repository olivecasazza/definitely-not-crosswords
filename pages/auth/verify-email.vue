<template>
  <div class="min-h-screen flex flex-col justify-center items-center px-4 bg-gradient-to-br from-[var(--bg-app)] via-[var(--bg-card)] to-[var(--bg-app)] relative overflow-hidden select-none">
    
    <!-- Background glow/blobs -->
    <div class="absolute top-1/4 left-1/4 w-96 h-96 bg-[var(--pastel-yellow)] opacity-10 rounded-full blur-[120px] pointer-events-none"></div>
    <div class="absolute bottom-1/4 right-1/4 w-96 h-96 bg-[var(--pastel-green)] opacity-10 rounded-full blur-[120px] pointer-events-none"></div>

    <div class="w-full max-w-md p-8 rounded-2xl border border-[var(--border-app)] bg-[rgba(24,24,27,0.7)] backdrop-blur-xl shadow-2xl relative z-10 text-center">
      
      <!-- Brand Logo & Header -->
      <div class="flex flex-col items-center mb-8">
        <div class="w-12 h-12 rounded-2xl bg-gradient-to-tr from-[var(--pastel-yellow)] to-[rgba(254,234,153,0.3)] flex items-center justify-center shadow-lg transform rotate-6 mb-4">
          <svg class="w-6 h-6 text-slate-900" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M3 19v-8.93a2 2 0 01.89-1.664l8-5.333a2 2 0 012.22 0l8 5.333A2 2 0 0121 10.07V19M3 19a2 2 0 002 2h14a2 2 0 002-2M3 19l6.75-4.5M21 19l-6.75-4.5M3 10l6.75 4.5M21 10l-6.75 4.5m0 0l-1.14.76a2 2 0 01-2.22 0l-1.14-.76" />
          </svg>
        </div>
        <h1 class="text-2xl font-bold font-mono tracking-wider text-[var(--text-primary)] uppercase">Email Verification</h1>
      </div>

      <!-- State: Verifying/Loading -->
      <div v-if="state === 'verifying'" class="space-y-6">
        <div class="flex justify-center">
          <div class="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-[var(--pastel-yellow)]"></div>
        </div>
        <p class="text-sm font-mono text-[var(--text-secondary)]">Verifying your email token...</p>
      </div>

      <!-- State: Success -->
      <div v-else-if="state === 'success'" class="space-y-6">
        <div class="flex justify-center">
          <div class="w-16 h-16 rounded-full bg-[rgba(168,230,207,0.1)] border-2 border-[var(--pastel-green)] flex items-center justify-center text-[var(--pastel-green)] shadow-lg shadow-[rgba(168,230,207,0.1)]">
            <svg class="w-8 h-8" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
            </svg>
          </div>
        </div>
        <h2 id="verification-success-title" class="text-lg font-bold text-[var(--pastel-green)] font-mono uppercase">Email Verified!</h2>
        <p class="text-xs text-[var(--text-secondary)] leading-relaxed">
          Your email address has been successfully verified. You can now log into the application.
        </p>
        <NuxtLink 
          to="/api/auth/signin"
          class="inline-block w-full py-3 px-4 rounded-xl font-semibold text-sm tracking-wider uppercase bg-gradient-to-r from-[var(--pastel-yellow)] to-[rgba(254,234,153,0.7)] text-slate-900 shadow-md hover:scale-[1.02] active:scale-[0.98] transition-all duration-300 cursor-pointer"
        >
          Sign In
        </NuxtLink>
      </div>

      <!-- State: Error -->
      <div v-else-if="state === 'error'" class="space-y-6">
        <div class="flex justify-center">
          <div class="w-16 h-16 rounded-full bg-[rgba(255,140,140,0.1)] border-2 border-[var(--pastel-red)] flex items-center justify-center text-[var(--pastel-red)] shadow-lg shadow-[rgba(255,140,140,0.1)]">
            <svg class="w-8 h-8" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </div>
        </div>
        <h2 class="text-lg font-bold text-[var(--pastel-red)] font-mono uppercase">Verification Failed</h2>
        <p class="text-xs text-[var(--text-secondary)] leading-relaxed">
          {{ errorMsg || "The verification token is invalid, expired, or has already been used." }}
        </p>
        <NuxtLink 
          to="/auth/signup"
          class="inline-block w-full py-3 px-4 rounded-xl font-semibold text-sm tracking-wider uppercase border border-[var(--border-app)] hover:border-[var(--border-hover)] text-[var(--text-primary)] hover:bg-[rgba(255,255,255,0.02)] shadow-md hover:scale-[1.02] active:scale-[0.98] transition-all duration-300 cursor-pointer"
        >
          Back to Signup
        </NuxtLink>
      </div>

    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from "vue";
import { useRoute } from "vue-router";

definePageMeta({
  auth: false,
});

const route = useRoute();
const { $client } = useNuxtApp();

const state = ref<"verifying" | "success" | "error">("verifying");
const errorMsg = ref("");

onMounted(async () => {
  const token = route.query.token as string | undefined;
  if (!token) {
    state.value = "error";
    errorMsg.value = "Missing verification token in URL query parameters.";
    return;
  }

  try {
    const res = await $client.user.verifyEmail.mutate({ token });
    if (res.success) {
      state.value = "success";
    }
  } catch (err: any) {
    state.value = "error";
    errorMsg.value = err.message || "Invalid or expired verification token.";
  }
});
</script>

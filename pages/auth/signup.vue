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
              @blur="touched.name = true"
              type="text"
              required
              placeholder="e.g. Olive Casazza"
              class="w-full px-4 py-3 rounded-xl border bg-[rgba(18,18,18,0.5)] focus:ring-1 outline-none text-sm transition-all duration-300"
              :class="[
                touched.name && validationErrors.name
                  ? 'border-[var(--pastel-red)] focus:border-[var(--pastel-red)] focus:ring-[var(--pastel-red)]'
                  : touched.name
                    ? 'border-[var(--pastel-green)] focus:border-[var(--pastel-green)] focus:ring-[var(--pastel-green)]'
                    : 'border-[var(--border-app)] focus:border-[var(--pastel-yellow)] focus:ring-[var(--pastel-yellow)]'
              ]"
            />
          </div>
          <p v-if="touched.name && validationErrors.name" class="text-[11px] text-[var(--pastel-red)] font-mono pl-1">
            {{ validationErrors.name }}
          </p>
        </div>

        <!-- Username Input -->
        <div class="space-y-1.5">
          <label for="username" class="block text-xs font-semibold uppercase tracking-wider text-[var(--text-secondary)] font-mono">Username</label>
          <div class="relative">
            <input
              id="username"
              v-model="form.username"
              @blur="touched.username = true"
              type="text"
              required
              placeholder="e.g. olivepasta"
              class="w-full px-4 py-3 rounded-xl border bg-[rgba(18,18,18,0.5)] focus:ring-1 outline-none text-sm transition-all duration-300"
              :class="[
                touched.username && (validationErrors.username || !usernameUnique)
                  ? 'border-[var(--pastel-red)] focus:border-[var(--pastel-red)] focus:ring-[var(--pastel-red)]'
                  : touched.username && usernameUnique && !checkingUsername && form.username.length >= 3
                    ? 'border-[var(--pastel-green)] focus:border-[var(--pastel-green)] focus:ring-[var(--pastel-green)]'
                    : 'border-[var(--border-app)] focus:border-[var(--pastel-yellow)] focus:ring-[var(--pastel-yellow)]'
              ]"
            />
            <!-- Inline spinner for uniqueness check -->
            <div v-if="checkingUsername" class="absolute right-3.5 top-3.5 flex items-center">
              <span class="animate-spin inline-block w-4 h-4 border-2 border-[var(--pastel-yellow)] border-t-transparent rounded-full"></span>
            </div>
          </div>
          <p v-if="touched.username && validationErrors.username" class="text-[11px] text-[var(--pastel-red)] font-mono pl-1">
            {{ validationErrors.username }}
          </p>
          <p v-else-if="touched.username && checkingUsername" class="text-[11px] text-[var(--text-secondary)] font-mono pl-1">
            Checking availability...
          </p>
          <p v-else-if="touched.username && !usernameUnique && form.username.length >= 3" class="text-[11px] text-[var(--pastel-red)] font-mono pl-1">
            Username is already taken.
          </p>
          <p v-else-if="touched.username && usernameUnique && form.username.length >= 3" class="text-[11px] text-[var(--pastel-green)] font-mono pl-1">
            Username is available!
          </p>
        </div>

        <!-- Email Input -->
        <div class="space-y-1.5">
          <label for="email" class="block text-xs font-semibold uppercase tracking-wider text-[var(--text-secondary)] font-mono">Email Address</label>
          <div class="relative">
            <input
              id="email"
              v-model="form.email"
              @blur="touched.email = true"
              type="email"
              required
              placeholder="e.g. olive.casazza@gmail.com"
              class="w-full px-4 py-3 rounded-xl border bg-[rgba(18,18,18,0.5)] focus:ring-1 outline-none text-sm transition-all duration-300"
              :class="[
                touched.email && (validationErrors.email || !emailUnique)
                  ? 'border-[var(--pastel-red)] focus:border-[var(--pastel-red)] focus:ring-[var(--pastel-red)]'
                  : touched.email && emailUnique && !checkingEmail && /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(form.email)
                    ? 'border-[var(--pastel-green)] focus:border-[var(--pastel-green)] focus:ring-[var(--pastel-green)]'
                    : 'border-[var(--border-app)] focus:border-[var(--pastel-yellow)] focus:ring-[var(--pastel-yellow)]'
              ]"
            />
            <!-- Inline spinner for uniqueness check -->
            <div v-if="checkingEmail" class="absolute right-3.5 top-3.5 flex items-center">
              <span class="animate-spin inline-block w-4 h-4 border-2 border-[var(--pastel-yellow)] border-t-transparent rounded-full"></span>
            </div>
          </div>
          <p v-if="touched.email && validationErrors.email" class="text-[11px] text-[var(--pastel-red)] font-mono pl-1">
            {{ validationErrors.email }}
          </p>
          <p v-else-if="touched.email && checkingEmail" class="text-[11px] text-[var(--text-secondary)] font-mono pl-1">
            Checking availability...
          </p>
          <p v-else-if="touched.email && !emailUnique && /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(form.email)" class="text-[11px] text-[var(--pastel-red)] font-mono pl-1">
            Email is already registered.
          </p>
          <p v-else-if="touched.email && emailUnique && /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(form.email)" class="text-[11px] text-[var(--pastel-green)] font-mono pl-1">
            Email is available!
          </p>
        </div>

        <!-- Password Input -->
        <div class="space-y-1.5">
          <label for="password" class="block text-xs font-semibold uppercase tracking-wider text-[var(--text-secondary)] font-mono">Password</label>
          <div class="relative">
            <input
              id="password"
              v-model="form.password"
              @blur="touched.password = true"
              type="password"
              required
              placeholder="••••••••"
              class="w-full px-4 py-3 rounded-xl border bg-[rgba(18,18,18,0.5)] focus:ring-1 outline-none text-sm transition-all duration-300"
              :class="[
                touched.password && validationErrors.password
                  ? 'border-[var(--pastel-red)] focus:border-[var(--pastel-red)] focus:ring-[var(--pastel-red)]'
                  : touched.password
                    ? 'border-[var(--pastel-green)] focus:border-[var(--pastel-green)] focus:ring-[var(--pastel-green)]'
                    : 'border-[var(--border-app)] focus:border-[var(--pastel-yellow)] focus:ring-[var(--pastel-yellow)]'
              ]"
            />
          </div>
          <p v-if="touched.password && validationErrors.password" class="text-[11px] text-[var(--pastel-red)] font-mono pl-1">
            {{ validationErrors.password }}
          </p>
        </div>

        <!-- Submit Button -->
        <button
          type="submit"
          :disabled="loading || isFormInvalid"
          class="w-full py-3 px-4 rounded-xl font-semibold text-sm tracking-wider uppercase bg-gradient-to-r from-[var(--pastel-yellow)] to-[rgba(254,234,153,0.7)] text-slate-900 shadow-md hover:scale-[1.02] active:scale-[0.98] disabled:opacity-40 transition-all duration-300 cursor-pointer flex items-center justify-center gap-2"
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
import { ref, reactive, computed, watch } from "vue";

definePageMeta({
  auth: false,
});

const { $client } = useNuxtApp();

const form = reactive({
  name: "",
  username: "",
  email: "",
  password: "",
});

const touched = reactive({
  name: false,
  username: false,
  email: false,
  password: false,
});

const loading = ref(false);
const error = ref("");
const success = ref(false);
const verificationToken = ref("");

// Uniqueness states
const checkingUsername = ref(false);
const usernameUnique = ref(true);
const checkingEmail = ref(false);
const emailUnique = ref(true);

// Debouncing timers
let usernameDebounceTimer: NodeJS.Timeout | null = null;
let emailDebounceTimer: NodeJS.Timeout | null = null;

// Clean reactive validation errors
const validationErrors = computed(() => {
  const errors = {
    name: "",
    username: "",
    email: "",
    password: "",
  };

  // Name validation
  if (!form.name) {
    errors.name = "Full Name is required.";
  } else if (form.name.trim().length < 2) {
    errors.name = "Full Name must be at least 2 characters.";
  }

  // Username validation
  if (!form.username) {
    errors.username = "Username is required.";
  } else if (form.username.trim().length < 3) {
    errors.username = "Username must be at least 3 characters.";
  }

  // Email validation
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  if (!form.email) {
    errors.email = "Email address is required.";
  } else if (!emailRegex.test(form.email)) {
    errors.email = "Please enter a valid email address.";
  }

  // Password validation
  if (!form.password) {
    errors.password = "Password is required.";
  } else if (form.password.length < 6) {
    errors.password = "Password must be at least 6 characters.";
  }

  return errors;
});

// Computed overall form validation state
const isFormInvalid = computed(() => {
  return (
    !!validationErrors.value.name ||
    !!validationErrors.value.username ||
    !!validationErrors.value.email ||
    !!validationErrors.value.password ||
    !usernameUnique.value ||
    !emailUnique.value ||
    checkingUsername.value ||
    checkingEmail.value
  );
});

// Debounced Server-side uniqueness check for Username
watch(() => form.username, (newUsername) => {
  if (usernameDebounceTimer) clearTimeout(usernameDebounceTimer);

  if (!newUsername || newUsername.trim().length < 3) {
    usernameUnique.value = true;
    checkingUsername.value = false;
    return;
  }

  checkingUsername.value = true;
  usernameDebounceTimer = setTimeout(async () => {
    try {
      const res = await $client.user.isUsernameUnique.query({ username: newUsername });
      usernameUnique.value = res.unique;
    } catch (err) {
      console.error("Failed to check username uniqueness:", err);
    } finally {
      checkingUsername.value = false;
    }
  }, 500); // 500ms debounce delay
});

// Debounced Server-side uniqueness check for Email
watch(() => form.email, (newEmail) => {
  if (emailDebounceTimer) clearTimeout(emailDebounceTimer);

  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  if (!newEmail || !emailRegex.test(newEmail)) {
    emailUnique.value = true;
    checkingEmail.value = false;
    return;
  }

  checkingEmail.value = true;
  emailDebounceTimer = setTimeout(async () => {
    try {
      const res = await $client.user.isEmailUnique.query({ email: newEmail });
      emailUnique.value = res.unique;
    } catch (err) {
      console.error("Failed to check email uniqueness:", err);
    } finally {
      checkingEmail.value = false;
    }
  }, 500); // 500ms debounce delay
});

async function handleSignup() {
  // Mark all fields touched
  touched.name = true;
  touched.username = true;
  touched.email = true;
  touched.password = true;

  if (isFormInvalid.value) return;

  loading.value = true;
  error.value = "";
  success.value = false;

  try {
    const res = await $client.user.signup.mutate({
      name: form.name,
      email: form.email,
      username: form.username,
      password: form.password,
    });

    if (res.success) {
      success.value = true;
      verificationToken.value = res.verificationToken;
      // Reset form
      form.name = "";
      form.username = "";
      form.email = "";
      form.password = "";
      // Reset touched state
      touched.name = false;
      touched.username = false;
      touched.email = false;
      touched.password = false;
    }
  } catch (err: any) {
    error.value = err.message || "An error occurred during signup.";
  } finally {
    loading.value = false;
  }
}
</script>

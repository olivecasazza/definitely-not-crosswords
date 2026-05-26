import themeColors from "./tailwind/colors.cjs";
import { theme } from "./tailwind/tailwind-workspace-preset";

// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  devtools: { enabled: false },
  extends: ['@sidebase/core'],
  modules: [
    '@sidebase/nuxt-auth',
    '@nuxtjs/tailwindcss',
    'nuxt-svgo',
    '@pinia/nuxt',
    '@vueuse/nuxt'
  ],
  runtimeConfig: {
    version: '0.0.1',
  },
  typescript: {
    shim: false
  },
  build: {
    transpile: ['trpc-nuxt']
  },
  nitro: {
    plugins: ['~/nitro/ws']
  }
})

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
    lemonSqueezy: {
      apiKey: '',
      storeId: '',
      variantId: '',
      webhookSecret: '',
    },
  },
  typescript: {
    shim: false
  },
  build: {
    transpile: ['trpc-nuxt', '@trpc/server']
  },
  nitro: {
    experimental: {
      websocket: true
    },
    externals: {
      trace: false
    },
    rollupConfig: {
      preserveSymlinks: true
    }
  },
  routeRules: {
    '/': { ssr: false },
    '/stats': { ssr: false },
    '/admin/**': { ssr: false },
    '/game/**': { ssr: false }
  },
  vite: {
    resolve: {
      preserveSymlinks: true
    },
    optimizeDeps: {
      include: ['@prisma/client']
    },
    server: {
      watch: {
        ignored: ['**/test-results/**', '**/playwright-report/**']
      }
    }
  }
})

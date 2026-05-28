import { createNuxtApiHandler } from 'trpc-nuxt';
import { createContext } from '~/server/trpc/context';
import { appRouter } from '~/server/trpc/router';

// export API handler
export default createNuxtApiHandler({
  router: appRouter,
  createContext,
})

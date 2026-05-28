import pkg from '@prisma/client'
import type { PrismaClient } from '@prisma/client'
const { PrismaClient } = pkg

let prisma: PrismaClient
declare module 'h3' {
  interface H3EventContext {
    prisma: PrismaClient
  }
}

export default eventHandler((event) => {
  if (!prisma) {
    prisma = new PrismaClient()
  }
  event.context.prisma = prisma
})

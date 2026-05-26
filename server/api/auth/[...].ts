import { NuxtAuthHandler } from "#auth";
import CredentialsProvider from "next-auth/providers/credentials";
import KeycloakProvider from "next-auth/providers/keycloak";

import { PrismaAdapter } from "@auth/prisma-adapter";
import pkg from "@prisma/client";
import type { PrismaClient } from "@prisma/client";
const { PrismaClient } = pkg;

const prisma = new PrismaClient();
const isProduction = process.env.NODE_ENV === "production";

export default NuxtAuthHandler({
  adapter: PrismaAdapter(prisma),
  secret: process.env.NEXTAUTH_SECRET || "supersecretsecret",
  session: {
    strategy: "jwt",
  },
  callbacks: {
    async jwt({ token, user }) {
      if (user?.email) {
        const dbUser = await prisma.user.findUnique({
          where: { email: user.email },
          select: { id: true, role: true },
        });
        token.id = dbUser?.id ?? user.id;
        token.role = dbUser?.role ?? "USER";
      }
      return token;
    },
    session({ session, token }) {
      if (session.user) {
        (session.user as typeof session.user & { id?: string; role?: string }).id = token.id as string | undefined;
        (session.user as typeof session.user & { role?: string }).role = token.role as string | undefined;
      }
      return session;
    },
  },
  providers: [
    ...(!isProduction
      ? [
          CredentialsProvider.default({
            id: "local-dev",
            name: "Local Dev",
            credentials: {
              email: {
                label: "Email",
                type: "email",
                value: process.env.LOCAL_ADMIN_EMAIL || "olive.casazza@gmail.com",
              },
            },
            async authorize(credentials) {
              const email = credentials?.email;
              if (!email) return null;

              const user = await prisma.user.findUnique({
                where: { email },
                select: { id: true, email: true, name: true, role: true },
              });

              if (!user) return null;

              return {
                id: user.id,
                email: user.email,
                name: user.name,
                role: user.role,
              };
            },
          }),
        ]
      : []),
    KeycloakProvider.default({
      clientId: process.env.KEYCLOAK_CLIENT_ID as string,
      clientSecret: process.env.KEYCLOAK_CLIENT_SECRET as string,
      issuer: process.env.KEYCLOAK_ISSUER as string,
    }),
  ],
});

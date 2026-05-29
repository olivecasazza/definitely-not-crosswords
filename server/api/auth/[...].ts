import { NuxtAuthHandler } from "#auth";
import CredentialsProvider from "next-auth/providers/credentials";

import { PrismaAdapter } from "@auth/prisma-adapter";
import pkg from "@prisma/client";
import type { PrismaClient } from "@prisma/client";
const { PrismaClient } = pkg;

const prisma = new PrismaClient();
const isProduction = process.env.NODE_ENV === "production" && process.env.E2E_TEST !== "true";
const stripAuthPathSuffix = (value: string) => value.replace(/\/api\/auth\/?$/, "");
const oidcProviderId = process.env.OIDC_PROVIDER_ID || "keycloak";
const oidcProviderName = process.env.OIDC_PROVIDER_NAME || "SSO";
const oidcIssuer = process.env.OIDC_ISSUER || process.env.KEYCLOAK_ISSUER;
const oidcClientId = process.env.OIDC_CLIENT_ID || process.env.KEYCLOAK_CLIENT_ID;
const oidcClientSecret = process.env.OIDC_CLIENT_SECRET || process.env.KEYCLOAK_CLIENT_SECRET;

const authHandler = NuxtAuthHandler({
  adapter: PrismaAdapter(prisma),
  secret: process.env.NEXTAUTH_SECRET || "supersecretsecret",
  trustHost: true,
  session: {
    strategy: "jwt",
  },
  callbacks: {
    async redirect({ url, baseUrl }) {
      const targetBase = stripAuthPathSuffix(process.env.NEXTAUTH_URL || baseUrl);
      if (url.startsWith("/")) {
        return `${targetBase}${url}`;
      }
      try {
        const parsedUrl = new URL(url);
        const parsedBase = new URL(targetBase);
        parsedUrl.host = parsedBase.host;
        parsedUrl.protocol = parsedBase.protocol;
        return parsedUrl.toString();
      } catch (e) {
        return targetBase;
      }
    },
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
    {
      id: oidcProviderId,
      name: oidcProviderName,
      type: "oauth",
      wellKnown: `${oidcIssuer}/.well-known/openid-configuration`,
      authorization: {
        params: {
          scope: "openid email profile",
        },
      },
      idToken: true,
      checks: ["pkce", "state"],
      clientId: oidcClientId as string,
      clientSecret: oidcClientSecret as string,
      profile(profile) {
        return {
          id: profile.sub,
          name: profile.name || profile.preferred_username || profile.email,
          email: profile.email,
          image: profile.picture,
        };
      },
      // Existing users (same verified email) can link to Keycloak on first OAuth login.
      allowDangerousEmailAccountLinking: true,
    },
  ],
});

export default defineEventHandler(async (event) => {
  // Dynamically bind next-auth and sidebase-auth origin to the active incoming host
  const host = event.node.req.headers.host;
  if (host) {
    const protocol = event.node.req.headers['x-forwarded-proto'] || 'http';
    const dynamicOrigin = stripAuthPathSuffix(`${protocol}://${host}`);
    process.env.NEXTAUTH_URL = dynamicOrigin;
    process.env.AUTH_ORIGIN = dynamicOrigin;
  }

  // If E2E testing is active and we receive a mock session request, fulfill it immediately in the backend
  if (process.env.E2E_TEST === "true" && event.node.req.url?.includes("/api/auth/session")) {
    const cookies = event.node.req.headers.cookie || "";
    if (cookies.includes("mock-session-token-value-for-testing")) {
      console.log("🎟️ Serving backend E2E mock session for Olive Casazza");
      return {
        user: {
          name: "Olive Casazza",
          email: "olive.casazza@gmail.com",
          role: "ADMIN"
        },
        expires: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString()
      };
    }
  }
  return authHandler(event);
});

import { NuxtAuthHandler } from "#auth";
import Auth0Provider from "next-auth/providers/auth0";

import { PrismaAdapter } from "@auth/prisma-adapter";
import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();

export default NuxtAuthHandler({
  adapter: PrismaAdapter(prisma),
  secret: "supersecretsecret",
  providers: [
    Auth0Provider.default({
      // origin: process.env.ORIGIN || "http://localhost:3000",
      clientId: process.env.AUTH0_CLIENT_ID as string,
      clientSecret: process.env.AUTH0_CLIENT_SECRET as string,
      issuer: process.env.AUTH0_ISSUER as string,
      // authorization: {
      //   params: {
      //     prompt: "consent",
      //     access_type: "offline",
      //     response_type: "code"
      //   }
      // }
    }),
  ],
});

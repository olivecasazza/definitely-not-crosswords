#!/usr/bin/env node
import { PrismaClient, UserRole } from "@prisma/client";

function parseAdminUsersConfig() {
  const raw = process.env.ADMIN_USERS_JSON;
  if (!raw) {
    return { users: [] };
  }

  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    throw new Error(`ADMIN_USERS_JSON must be valid JSON: ${error.message}`);
  }

  if (Array.isArray(parsed)) {
    return { users: parsed };
  }

  if (!parsed || typeof parsed !== "object") {
    throw new Error("ADMIN_USERS_JSON must be an object with a users array.");
  }

  return { users: Array.isArray(parsed.users) ? parsed.users : [] };
}

function normalizeSeedUser(user, index) {
  if (!user || typeof user !== "object") {
    throw new Error(`adminUsers.users[${index}] must be an object.`);
  }

  const email = typeof user.email === "string" ? user.email.trim().toLowerCase() : "";
  if (!email) {
    throw new Error(`adminUsers.users[${index}].email is required.`);
  }

  const role = typeof user.role === "string" ? user.role.trim().toUpperCase() : "ADMIN";
  if (!Object.prototype.hasOwnProperty.call(UserRole, role)) {
    throw new Error(
      `adminUsers.users[${index}].role must be one of: ${Object.keys(UserRole).join(", ")}.`
    );
  }

  const name = typeof user.name === "string" && user.name.trim() ? user.name.trim() : null;
  const emailVerified = user.emailVerified !== false;

  return { email, name, role, emailVerified };
}

async function seedAdminUsers() {
  const config = parseAdminUsersConfig();
  const users = config.users.map(normalizeSeedUser);

  if (users.length === 0) {
    console.log("No admin users configured; skipping admin user seed.");
    return;
  }

  const prisma = new PrismaClient();

  try {
    for (const user of users) {
      const existing = await prisma.user.findUnique({
        where: { email: user.email },
        select: { id: true, emailVerified: true, name: true, role: true },
      });

      const verifiedAt = user.emailVerified ? existing?.emailVerified ?? new Date() : undefined;

      if (!existing) {
        await prisma.user.create({
          data: {
            email: user.email,
            name: user.name,
            role: user.role,
            emailVerified: verifiedAt,
          },
        });
        console.log(`Created seeded user ${user.email} with role ${user.role}.`);
        continue;
      }

      const updateData = {};
      if (existing.role !== user.role) {
        updateData.role = user.role;
      }
      if (user.name && existing.name !== user.name) {
        updateData.name = user.name;
      }
      if (verifiedAt && !existing.emailVerified) {
        updateData.emailVerified = verifiedAt;
      }

      if (Object.keys(updateData).length > 0) {
        await prisma.user.update({
          where: { email: user.email },
          data: updateData,
        });
        console.log(`Updated seeded user ${user.email}.`);
      } else {
        console.log(`Seeded user ${user.email} already exists with requested role.`);
      }
    }
  } finally {
    await prisma.$disconnect();
  }
}

seedAdminUsers().catch((error) => {
  console.error(error);
  process.exit(1);
});

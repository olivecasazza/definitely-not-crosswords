export const roleCapabilities = {
  USER: ["game:play", "profile:manage"],
  ADMIN: ["game:play", "profile:manage", "admin:access", "generator:manage"],
} as const;

export type AppRole = keyof typeof roleCapabilities;
export type AppCapability = (typeof roleCapabilities)[AppRole][number];

export const appRoles = Object.keys(roleCapabilities) as [AppRole, ...AppRole[]];

export function isAppRole(role: string | null | undefined): role is AppRole {
  return !!role && Object.prototype.hasOwnProperty.call(roleCapabilities, role);
}

export function roleHasCapability(
  role: string | null | undefined,
  capability: AppCapability
): boolean {
  return isAppRole(role) && roleCapabilities[role].includes(capability as never);
}

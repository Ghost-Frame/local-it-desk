import type { UserRole } from "@/types/api";

/** Returns whether a role grants access to administrator-only controls. */
export function canAccessAdministratorControls(role: UserRole | null | undefined): boolean {
  return role === "administrator";
}

import type { UserRole } from "@/types/api";

/** Returns whether a role grants access to Administration. */
export function canAccessAdministration(role: UserRole | null | undefined): boolean {
  return role === "administrator";
}

/** Pure account-administration decisions shared by UI and contract tests. */

import { ApiError } from "./api.js";
import type { OneTimeCredential, User } from "../types/api.js";

/** Determines whether the target is the only currently active administrator. */
export function isFinalActiveAdministrator(users: User[], targetId: string): boolean {
  const target = users.find((user) => user.id === targetId);
  if (!target || target.role !== "administrator" || !target.is_active) return false;
  return users.filter((user) => user.role === "administrator" && user.is_active).length === 1;
}

/** Extracts a safe server error message while retaining a useful fallback. */
export function accountErrorMessage(error: unknown, fallback: string): string {
  if (!(error instanceof ApiError) || typeof error.body !== "object" || error.body === null) {
    return fallback;
  }
  const body = error.body as { error?: unknown };
  return typeof body.error === "string" && body.error.length <= 240 ? body.error : fallback;
}

/** Formats transient onboarding material for the operator clipboard or printout. */
export function credentialsText(credentials: OneTimeCredential[]): string {
  return credentials
    .map(
      (entry) =>
        `${entry.user.display_name}\nUsername: ${entry.user.username}\nTemporary password: ${entry.temporary_password}`,
    )
    .join("\n\n");
}

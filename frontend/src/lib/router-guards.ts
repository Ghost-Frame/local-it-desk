/** Pure navigation policy for setup, local sessions, and account roles. */

import type { UserRole } from "../types/api.js";

/** Minimal route shape required by the authentication guard. */
export interface GuardRoute {
  /** Canonical route path without its query string. */
  path: string;
  /** Full local destination preserved after successful login. */
  redirectPath?: string;
  /** Whether the destination requires a valid local session. */
  requiresAuth: boolean;
  /** Whether the destination additionally requires an administrator. */
  requiresAdministrator?: boolean;
  /** Whether the destination requires a technician or administrator. */
  requiresWorker?: boolean;
}

/** Current authentication facts used by the pure route policy. */
export interface AuthGuardState {
  /** Whether the empty database still needs its first administrator. */
  setupRequired: boolean;
  /** Whether a server-managed session resolved a current account. */
  isAuthenticated: boolean;
  /** Whether normal product access is blocked until password replacement. */
  mustChangePassword: boolean;
  /** Whether the current account holds the administrator role. */
  isAdministrator: boolean;
  /** Whether the current account may work the shared ticket queue. */
  canWorkTickets?: boolean;
  /** Current account role used for a direct work landing. */
  userRole?: UserRole;
}

/** Returns the active workspace for one authenticated account role. */
export function landingPath(role: UserRole | null | undefined): "/tickets" | "/administration" {
  return role === "administrator" || role === "technician" ? "/administration" : "/tickets";
}

/** Returns a same-origin application path or a safe dashboard fallback. */
export function safePostLoginPath(candidate: unknown): string {
  if (typeof candidate !== "string") return "/";
  if (!candidate.startsWith("/") || candidate.startsWith("//")) return "/";
  if (candidate.includes("\\") || /[\u0000-\u001f\u007f]/.test(candidate)) return "/";
  if (["/login", "/setup", "/change-password"].includes(candidate)) return "/";
  return candidate;
}

/** Converts the former boolean guard input into a complete compatibility state. */
function normalizeState(state: AuthGuardState | boolean): AuthGuardState {
  if (typeof state !== "boolean") return state;
  return {
    setupRequired: false,
    isAuthenticated: state,
    mustChangePassword: false,
    isAdministrator: false,
    canWorkTickets: false,
    userRole: "requester",
  };
}

/** Resolves one redirect while preventing setup, login, and forced-change loops. */
export function resolveGuardRedirect(
  to: GuardRoute,
  suppliedState: AuthGuardState | boolean,
): string | undefined {
  const state = normalizeState(suppliedState);
  const role = state.userRole ?? (state.isAdministrator ? "administrator" : "requester");
  const activeLanding = landingPath(role);

  if (state.setupRequired) {
    return to.path === "/setup" ? undefined : "/setup";
  }
  if (to.path === "/setup") {
    return state.isAuthenticated ? activeLanding : "/login";
  }
  if (!state.isAuthenticated) {
    if (to.path === "/login") return undefined;
    if (!to.requiresAuth) return undefined;
    const intended = safePostLoginPath(to.redirectPath ?? to.path);
    return `/login?redirect=${encodeURIComponent(intended)}`;
  }
  if (state.mustChangePassword) {
    return to.path === "/change-password" ? undefined : "/change-password";
  }
  if (["/login", "/change-password"].includes(to.path)) return activeLanding;
  if (to.requiresAdministrator && !state.isAdministrator) return activeLanding;
  if (to.requiresWorker && !(state.canWorkTickets ?? state.isAdministrator)) return activeLanding;
  if (to.path === "/") return activeLanding;
  return undefined;
}

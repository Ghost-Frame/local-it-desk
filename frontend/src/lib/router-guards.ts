/** Pure navigation policy for setup, local sessions, and account roles. */

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
  };
}

/** Resolves one redirect while preventing setup, login, and forced-change loops. */
export function resolveGuardRedirect(
  to: GuardRoute,
  suppliedState: AuthGuardState | boolean,
): string | undefined {
  const state = normalizeState(suppliedState);

  if (state.setupRequired) {
    return to.path === "/setup" ? undefined : "/setup";
  }
  if (to.path === "/setup") {
    return state.isAuthenticated ? "/" : "/login";
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
  if (["/login", "/change-password"].includes(to.path)) return "/";
  if (to.requiresAdministrator && !state.isAdministrator) return "/";
  return undefined;
}

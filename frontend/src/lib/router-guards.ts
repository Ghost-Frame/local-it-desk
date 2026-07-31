/**
 * router-guards.ts
 *
 * Pure navigation-guard helper extracted from the router so it can be unit-
 * tested under Node without a DOM or Pinia store.
 *
 * The function is intentionally free of side effects: it returns a redirect
 * target or undefined (allow) and leaves all routing decisions to the caller.
 */

/** Minimal route shape the guard needs to make its decision. */
export interface GuardRoute {
  /** The path being navigated to. */
  path: string;
  /** True if the route requires an authenticated user. */
  requiresAuth: boolean;
}

/**
 * Decide whether a navigation should be redirected.
 *
 * Rules:
 * - If the destination requires auth and the user is NOT authenticated,
 *   redirect to /login preserving the intended path as ?redirect=<path>.
 *   Precondition: /login must NOT be marked requiresAuth=true in the route
 *   definition; if it were, this branch would redirect /login -> /login and
 *   loop. The guard defends against that by skipping the redirect when the
 *   destination is already /login.
 * - If the destination is /login and the user IS authenticated,
 *   redirect to / (already logged in).
 * - Otherwise, allow the navigation (return undefined).
 *
 * @param to       - The route being navigated to.
 * @param isAuthed - Whether the current user has a valid session.
 * @returns A redirect path string, or undefined to allow the navigation.
 */
export function resolveGuardRedirect(
  to: GuardRoute,
  isAuthed: boolean,
): string | undefined {
  if (to.requiresAuth && !isAuthed) {
    /**
     * Self-redirect guard: if the destination is already /login, allow it
     * through instead of producing a /login?redirect=%2Flogin loop.
     * This handles the misconfigured-route case defensively.
     */
    if (to.path === "/login") return undefined;

    /** Encode the intended destination for post-login redirect. */
    const redirect = encodeURIComponent(to.path);
    return `/login?redirect=${redirect}`;
  }

  if (to.path === "/login" && isAuthed) {
    /** Already authenticated -- bounce to app root. */
    return "/";
  }

  return undefined;
}

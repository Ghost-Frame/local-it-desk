/** Vue Router configuration for the approved local help-desk pages. */

import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";

import { canAccessAdministration } from "@/lib/admin-guard";
import { resolveGuardRedirect } from "@/lib/router-guards";

/** Approved browser routes for the foundation product. */
const routes: RouteRecordRaw[] = [
  { path: "/login", name: "login", component: () => import("@/views/LoginView.vue") },
  {
    path: "/",
    name: "dashboard",
    component: () => import("@/views/HomeView.vue"),
    meta: { requiresAuth: true },
  },
  {
    path: "/tickets",
    name: "tickets",
    component: () => import("@/views/TicketsView.vue"),
    meta: { requiresAuth: true },
  },
  {
    path: "/tickets/:id",
    name: "ticket",
    component: () => import("@/views/TicketsView.vue"),
    meta: { requiresAuth: true },
  },
  {
    path: "/announcements",
    name: "announcements",
    component: () => import("@/views/AnnouncementsView.vue"),
    meta: { requiresAuth: true },
  },
  {
    path: "/settings",
    name: "settings",
    component: () => import("@/views/SettingsView.vue"),
    meta: { requiresAuth: true },
  },
  {
    path: "/administration",
    name: "administration",
    component: () => import("@/views/AdminView.vue"),
    meta: { requiresAuth: true, requiresAdministrator: true },
  },
];

/** Application router using normal history for same-origin server fallback. */
export const router = createRouter({ history: createWebHistory(), routes });

/** Enforces cookie-session and administrator route boundaries. */
router.beforeEach(async (to) => {
  const { useAuthStore } = await import("@/stores/auth");
  const auth = useAuthStore();
  await auth.init();

  const redirect = resolveGuardRedirect(
    { path: to.path, requiresAuth: to.meta.requiresAuth === true },
    auth.isAuthenticated,
  );
  if (redirect) return redirect;
  if (
    to.meta.requiresAdministrator === true &&
    !canAccessAdministration(auth.user?.role)
  ) {
    return { name: "dashboard" };
  }
  return true;
});

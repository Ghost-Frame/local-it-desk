/** Vue Router configuration for the approved local help-desk pages. */

import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";

import { resolveGuardRedirect } from "@/lib/router-guards";

/** Approved browser routes for the foundation product. */
const routes: RouteRecordRaw[] = [
  { path: "/setup", name: "setup", component: () => import("@/views/SetupView.vue") },
  { path: "/login", name: "login", component: () => import("@/views/LoginView.vue") },
  {
    path: "/change-password",
    name: "change-password",
    component: () => import("@/views/ChangePasswordView.vue"),
    meta: { requiresAuth: true },
  },
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
    path: "/announcements/:id",
    name: "announcement",
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
    {
      path: to.path,
      redirectPath: to.fullPath,
      requiresAuth: to.meta.requiresAuth === true,
      requiresAdministrator: to.meta.requiresAdministrator === true,
    },
    {
      setupRequired: auth.setupRequired,
      isAuthenticated: auth.isAuthenticated,
      mustChangePassword: auth.mustChangePassword,
      isAdministrator: auth.isAdministrator,
    },
  );
  if (redirect) return redirect;
  return true;
});

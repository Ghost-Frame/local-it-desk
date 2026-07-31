/** Cookie-session authentication state for the local help desk. */

import { computed, ref } from "vue";
import { defineStore } from "pinia";

import { api } from "@/lib/api";
import type { LoginRequest, PublicConfig, User } from "@/types/api";

/** Shared authentication and public-configuration store. */
export const useAuthStore = defineStore("auth", () => {
  const user = ref<User | null>(null);
  const publicConfig = ref<PublicConfig | null>(null);
  const isLoading = ref(true);
  let initPromise: Promise<void> | null = null;

  /** Whether a server-managed session resolved an account. */
  const isAuthenticated = computed(() => user.value !== null);
  /** Whether the current role can manage shared tickets. */
  const canWorkTickets = computed(
    () => user.value?.role === "technician" || user.value?.role === "administrator",
  );
  /** Whether the current role can access Administration. */
  const isAdministrator = computed(() => user.value?.role === "administrator");
  /** Display name used by the application shell. */
  const displayName = computed(() => user.value?.display_name ?? "Signed out");

  /** Loads public configuration and attempts to resolve an existing cookie session once. */
  async function init(): Promise<void> {
    if (initPromise) return initPromise;
    initPromise = initialize();
    return initPromise;
  }

  /** Performs the first-load requests without exposing authentication details. */
  async function initialize(): Promise<void> {
    try {
      publicConfig.value = await api.getPublicConfig();
      try {
        user.value = await api.getCurrentUser();
      } catch {
        user.value = null;
      }
    } finally {
      isLoading.value = false;
    }
  }

  /** Starts a local account session and stores only the returned public user. */
  async function login(credentials: LoginRequest): Promise<void> {
    user.value = await api.login(credentials);
  }

  /** Ends the local account session and clears browser identity state. */
  async function logout(): Promise<void> {
    try {
      await api.logout();
    } finally {
      user.value = null;
    }
  }

  return {
    user,
    publicConfig,
    isLoading,
    isAuthenticated,
    canWorkTickets,
    isAdministrator,
    displayName,
    init,
    login,
    logout,
  };
});

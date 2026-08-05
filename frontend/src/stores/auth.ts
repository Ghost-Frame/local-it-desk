/** In-memory identity state for server-managed local account sessions. */

import { computed, ref } from "vue";
import { defineStore } from "pinia";

import { api } from "@/lib/api";
import type {
  ChangePasswordRequest,
  LoginRequest,
  PublicConfig,
  SetupRequest,
  User,
} from "@/types/api";

/** Shared authentication and public-configuration store. */
export const useAuthStore = defineStore("auth", () => {
  const user = ref<User | null>(null);
  const publicConfig = ref<PublicConfig | null>(null);
  /** In-memory cache revision changed after each successful branding refresh. */
  const brandingRevision = ref(0);
  const isLoading = ref(true);
  let initPromise: Promise<void> | null = null;

  /** Whether a server-managed session resolved an account. */
  const isAuthenticated = computed(() => user.value !== null);
  /** Whether first-run administrator setup is still required. */
  const setupRequired = computed(() => publicConfig.value?.setup_required === true);
  /** Whether product access remains blocked pending password replacement. */
  const mustChangePassword = computed(() => user.value?.must_change_password === true);
  /** Whether the current role can manage shared tickets. */
  const canWorkTickets = computed(
    () => user.value?.role === "technician" || user.value?.role === "administrator",
  );
  /** Whether the current role can access administrator-only controls. */
  const isAdministrator = computed(() => user.value?.role === "administrator");
  /** Display name used by the application shell. */
  const displayName = computed(() => user.value?.display_name ?? "Signed out");
  /** Cache-busted same-origin logo URL for the current runtime branding revision. */
  const logoUrl = computed(() => {
    const source = publicConfig.value?.logo_url;
    return source ? source + "?v=" + brandingRevision.value : null;
  });

  /** Accepts public configuration and updates browser-visible runtime branding. */
  function acceptPublicConfig(config: PublicConfig): void {
    publicConfig.value = config;
    brandingRevision.value = Date.now();
    if (typeof document !== "undefined") document.title = config.app_name;
  }

  /** Reloads non-secret configuration after administrator runtime changes. */
  async function refreshPublicConfig(): Promise<PublicConfig> {
    /** Fresh configuration also gives callers a strictly non-null snapshot. */
    const config = await api.getPublicConfig();
    acceptPublicConfig(config);
    return config;
  }

  /** Loads public configuration and resolves an existing cookie session once. */
  async function init(): Promise<void> {
    if (initPromise) return initPromise;
    initPromise = initialize();
    return initPromise;
  }

  /** Performs first-load requests without exposing authentication details. */
  async function initialize(): Promise<void> {
    try {
      /** Non-null initialization snapshot avoids reasoning through mutable ref state. */
      const config = await refreshPublicConfig();
      if (config.setup_required) {
        clearIdentity();
        return;
      }
      try {
        user.value = await api.getCurrentSession();
      } catch {
        clearIdentity();
      }
    } finally {
      isLoading.value = false;
    }
  }

  /** Creates the first local administrator and enters its new session. */
  async function setup(details: SetupRequest): Promise<void> {
    user.value = await api.setup(details);
    if (publicConfig.value) publicConfig.value.setup_required = false;
  }

  /** Starts a local account session and retains only public identity state. */
  async function login(credentials: LoginRequest): Promise<void> {
    user.value = await api.login(credentials);
  }

  /** Replaces the current password and accepts the server-rotated session. */
  async function changePassword(details: ChangePasswordRequest): Promise<void> {
    user.value = await api.changePassword(details);
  }

  /** Ends the local account session and clears browser identity state. */
  async function logout(): Promise<void> {
    try {
      await api.logout();
    } finally {
      clearIdentity();
    }
  }

  /** Forgets a session that an administrator action already revoked server-side. */
  function forgetSession(): void {
    clearIdentity();
  }

  /** Clears public identity and transient request-integrity state together. */
  function clearIdentity(): void {
    user.value = null;
    api.clearAuthentication();
  }

  return {
    user,
    publicConfig,
    isLoading,
    isAuthenticated,
    setupRequired,
    mustChangePassword,
    canWorkTickets,
    isAdministrator,
    displayName,
    logoUrl,
    init,
    setup,
    login,
    changePassword,
    logout,
    forgetSession,
    refreshPublicConfig,
  };
});

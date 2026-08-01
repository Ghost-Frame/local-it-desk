<script setup lang="ts">
/** Administrator workspace for named accounts, sessions, rosters, and audit history. */
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";

import AuditLogTable from "@/components/admin/AuditLogTable.vue";
import OnboardingPanel from "@/components/admin/OnboardingPanel.vue";
import RosterImport from "@/components/admin/RosterImport.vue";
import UserEditor from "@/components/admin/UserEditor.vue";
import UserRow from "@/components/admin/UserRow.vue";
import AppLayout from "@/components/layout/AppLayout.vue";
import { accountErrorMessage, isFinalActiveAdministrator } from "@/lib/account-admin";
import { api } from "@/lib/api";
import { useAuthStore } from "@/stores/auth";
import type { AuditEntry, CreateUserRequest, OneTimeCredential, UpdateUserRequest, User } from "@/types/api";

/** Approved administrator tabs. */
type AdminTab = "staff" | "roster" | "sessions" | "audit";

/** Stable tab definitions for keyboard and screen-reader navigation. */
const tabs: Array<{ id: AdminTab; label: string }> = [
  { id: "staff", label: "Staff" },
  { id: "roster", label: "Roster import" },
  { id: "sessions", label: "Sessions" },
  { id: "audit", label: "Audit" },
];

/** Client-side router used after a self-access change or session revocation. */
const router = useRouter();
/** Current local authentication state. */
const authStore = useAuthStore();
/** Currently selected administration area. */
const activeTab = ref<AdminTab>("staff");
/** Current bounded account page. */
const users = ref<User[]>([]);
/** Current bounded audit page. */
const auditEntries = ref<AuditEntry[]>([]);
/** One-time credentials retained only until explicit dismissal. */
const onboardingCredentials = ref<OneTimeCredential[]>([]);
/** Account operation identifier used to prevent duplicate submissions. */
const busyUserId = ref<string | null>(null);
/** Whether account listing is loading. */
const loadingUsers = ref(true);
/** Whether audit history is loading. */
const loadingAudit = ref(false);
/** Safe operation feedback shown to the administrator. */
const message = ref("");
/** Whether current role state permits rendering administrator controls. */
const authorized = computed(() => authStore.isAdministrator);

/** Loads the current bounded page of account records. */
async function loadUsers(): Promise<void> {
  loadingUsers.value = true;
  try {
    users.value = (await api.listUsers()).items;
  } catch (error) {
    message.value = accountErrorMessage(error, "Staff accounts could not be loaded.");
  } finally {
    loadingUsers.value = false;
  }
}

/** Loads the newest bounded page of privacy-safe audit records. */
async function loadAudit(): Promise<void> {
  loadingAudit.value = true;
  try {
    auditEntries.value = (await api.listAuditEntries()).items;
  } catch (error) {
    message.value = accountErrorMessage(error, "Audit history could not be loaded.");
  } finally {
    loadingAudit.value = false;
  }
}

/** Selects one tab and lazily refreshes its server data. */
function selectTab(tab: AdminTab): void {
  activeTab.value = tab;
  message.value = "";
  if (tab === "audit") void loadAudit();
}

/** Creates one account and exposes its password only in transient UI state. */
async function createAccount(details: CreateUserRequest): Promise<void> {
  busyUserId.value = "create";
  message.value = "";
  try {
    onboardingCredentials.value = [await api.createUser(details)];
    await loadUsers();
  } catch (error) {
    message.value = accountErrorMessage(error, "The account could not be created.");
  } finally {
    busyUserId.value = null;
  }
}

/** Updates one account and applies any self-role change to local navigation state. */
async function updateAccount(account: User, details: UpdateUserRequest): Promise<void> {
  busyUserId.value = account.id;
  message.value = "";
  try {
    const updatesCurrentAccount = account.id === authStore.user?.id;
    const updated = await api.updateUser(account.id, details);
    if (updatesCurrentAccount) authStore.user = updated;
    if (!updated.is_active) {
      authStore.forgetSession();
      await router.replace("/login");
      return;
    }
    if (updatesCurrentAccount && updated.role !== "administrator") {
      await router.replace("/");
      return;
    }
    await loadUsers();
    message.value = "Account updated.";
  } catch (error) {
    message.value = accountErrorMessage(error, "The account could not be updated.");
  } finally {
    busyUserId.value = null;
  }
}

/** Resets another account and exposes its replacement password once. */
async function resetPassword(account: User): Promise<void> {
  busyUserId.value = account.id;
  message.value = "";
  try {
    onboardingCredentials.value = [await api.resetUserPassword(account.id)];
    await loadUsers();
  } catch (error) {
    message.value = accountErrorMessage(error, "The password could not be reset.");
  } finally {
    busyUserId.value = null;
  }
}

/** Revokes all sessions and signs out when the administrator targets themself. */
async function revokeSessions(account: User): Promise<void> {
  busyUserId.value = account.id;
  message.value = "";
  try {
    await api.revokeUserSessions(account.id);
    if (account.id === authStore.user?.id) {
      authStore.forgetSession();
      await router.replace("/login");
      return;
    }
    message.value = `Sessions revoked for ${account.display_name}.`;
  } catch (error) {
    message.value = accountErrorMessage(error, "Sessions could not be revoked.");
  } finally {
    busyUserId.value = null;
  }
}

/** Accepts one successful roster result and refreshes the staff list. */
function rosterImported(credentials: OneTimeCredential[]): void {
  onboardingCredentials.value = credentials;
  void loadUsers();
}

/** Clears one-time onboarding material from application memory. */
function dismissOnboarding(): void {
  onboardingCredentials.value = [];
}

onMounted(() => {
  if (authStore.isAdministrator) void loadUsers();
});
</script>

<template>
  <AppLayout>
    <section v-if="!authorized" class="rounded-2xl border bg-[var(--color-surface-secondary)] p-6" :style="{ borderColor: 'var(--color-border-default)' }" role="alert">
      <h1 class="text-2xl font-bold">Administrator access required</h1>
      <p class="mt-2 text-[var(--color-text-secondary)]">Your account cannot manage local staff accounts.</p>
    </section>

    <section v-else class="space-y-6">
      <header class="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
        <div><p class="font-mono text-xs uppercase tracking-[0.2em] text-[var(--color-accent-primary)]">Local operator console</p><h1 class="mt-3 text-4xl font-bold tracking-tight">Administration</h1><p class="mt-3 max-w-2xl text-[var(--color-text-secondary)]">Manage named staff access without external email or identity services.</p></div>
        <p class="rounded-lg bg-[var(--color-surface-tertiary)] px-4 py-3 text-sm"><strong>{{ users.length }}</strong> local accounts</p>
      </header>

      <OnboardingPanel v-if="onboardingCredentials.length" :credentials="onboardingCredentials" @dismiss="dismissOnboarding" />
      <p v-if="message" class="rounded-lg border p-3 text-sm" :style="{ borderColor: 'var(--color-border-default)' }" role="status">{{ message }}</p>

      <nav class="flex gap-1 overflow-x-auto border-b" :style="{ borderColor: 'var(--color-border-default)' }" role="tablist" aria-label="Administration sections">
        <button v-for="tab in tabs" :id="`tab-${tab.id}`" :key="tab.id" class="min-h-11 shrink-0 border-b-2 px-4 text-sm font-bold" :class="activeTab === tab.id ? 'border-[var(--color-accent-primary)] text-[var(--color-accent-primary)]' : 'border-transparent text-[var(--color-text-secondary)]'" role="tab" :aria-selected="activeTab === tab.id" :aria-controls="`panel-${tab.id}`" @click="selectTab(tab.id)">{{ tab.label }}</button>
      </nav>

      <div v-if="activeTab === 'staff'" id="panel-staff" class="space-y-5" role="tabpanel" aria-labelledby="tab-staff">
        <UserEditor :busy="busyUserId === 'create'" @create="createAccount" />
        <p v-if="loadingUsers" role="status">Loading staff accounts…</p>
        <div v-else class="grid gap-3"><UserRow v-for="account in users" :key="account.id" :user="account" :current-user-id="authStore.user?.id ?? ''" :final-active-administrator="isFinalActiveAdministrator(users, account.id)" :busy="busyUserId === account.id" @update="updateAccount" @reset="resetPassword" @revoke="revokeSessions" /></div>
      </div>

      <div v-else-if="activeTab === 'roster'" id="panel-roster" role="tabpanel" aria-labelledby="tab-roster"><RosterImport @imported="rosterImported" /></div>

      <div v-else-if="activeTab === 'sessions'" id="panel-sessions" class="space-y-3" role="tabpanel" aria-labelledby="tab-sessions">
        <div class="rounded-2xl bg-[var(--color-surface-secondary)] p-5"><h2 class="text-xl font-bold">Active session controls</h2><p class="mt-1 text-sm text-[var(--color-text-secondary)]">Use this after a lost device, suspected account compromise, or staff departure.</p></div>
        <UserRow v-for="account in users" :key="account.id" :user="account" :current-user-id="authStore.user?.id ?? ''" :final-active-administrator="false" :busy="busyUserId === account.id" sessions-only @update="updateAccount" @reset="resetPassword" @revoke="revokeSessions" />
      </div>

      <div v-else id="panel-audit" role="tabpanel" aria-labelledby="tab-audit"><AuditLogTable :entries="auditEntries" :loading="loadingAudit" /></div>
    </section>
  </AppLayout>
</template>

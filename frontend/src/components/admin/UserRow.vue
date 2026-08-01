<script setup lang="ts">
/** Editable account row with explicit confirmation for security-sensitive actions. */
import { computed, nextTick, ref, watch } from "vue";

import type { UpdateUserRequest, User, UserRole } from "@/types/api";

/** Security-sensitive actions that require a modal confirmation. */
type ConfirmationAction = "self-update" | "reset-password" | "revoke-sessions";

const props = defineProps<{
  /** Account displayed by this row. */
  user: User;
  /** Signed-in administrator identifier. */
  currentUserId: string;
  /** Whether demotion or deactivation would remove the last active administrator. */
  finalActiveAdministrator: boolean;
  /** Whether the parent is processing an account operation. */
  busy: boolean;
  /** Whether the row is displayed in session-management mode. */
  sessionsOnly?: boolean;
}>();

const emit = defineEmits<{
  /** Requests an account metadata or privilege update. */
  update: [user: User, details: UpdateUserRequest];
  /** Requests a one-time password reset for another account. */
  reset: [user: User];
  /** Requests revocation of every active session for one account. */
  revoke: [user: User];
}>();

/** Editable display-name draft. */
const displayName = ref(props.user.display_name);
/** Editable role draft. */
const role = ref<UserRole>(props.user.role);
/** Editable activation draft. */
const isActive = ref(props.user.is_active);
/** Current password supplied only for a signed-in administrator privilege change. */
const currentPassword = ref("");
/** Native confirmation dialog instance. */
const confirmationDialog = ref<HTMLDialogElement | null>(null);
/** Pending action represented by the dialog. */
const confirmationAction = ref<ConfirmationAction | null>(null);
/** Control that opened the dialog, restored after close. */
const actionTrigger = ref<HTMLElement | null>(null);

/** Whether this row represents the signed-in administrator. */
const isSelf = computed(() => props.user.id === props.currentUserId);
/** Whether the draft changes role or activation state. */
const privilegeChanged = computed(() => role.value !== props.user.role || isActive.value !== props.user.is_active);
/** Whether any editable field differs from persisted state. */
const dirty = computed(() => displayName.value.trim() !== props.user.display_name || privilegeChanged.value);

/** Restores editable drafts after parent data refresh. */
function resetDraft(): void {
  displayName.value = props.user.display_name;
  role.value = props.user.role;
  isActive.value = props.user.is_active;
}

/** Opens one modal and remembers where keyboard focus must return. */
async function openConfirmation(action: ConfirmationAction, event: Event): Promise<void> {
  confirmationAction.value = action;
  actionTrigger.value = event.currentTarget as HTMLElement;
  await nextTick();
  confirmationDialog.value?.showModal();
}

/** Closes the modal, clears sensitive input, and restores trigger focus. */
async function closeConfirmation(): Promise<void> {
  confirmationDialog.value?.close();
  confirmationAction.value = null;
  currentPassword.value = "";
  await nextTick();
  actionTrigger.value?.focus();
}

/** Saves harmless metadata directly and confirms changes to the current administrator. */
function requestSave(event: Event): void {
  if (!dirty.value) return;
  if (isSelf.value && privilegeChanged.value) {
    void openConfirmation("self-update", event);
    return;
  }
  emit("update", props.user, { display_name: displayName.value.trim(), role: role.value, is_active: isActive.value });
}

/** Executes the action represented by the open confirmation dialog. */
function confirmAction(): void {
  if (confirmationAction.value === "self-update") {
    emit("update", props.user, { display_name: displayName.value.trim(), role: role.value, is_active: isActive.value, current_password: currentPassword.value });
  } else if (confirmationAction.value === "reset-password") {
    emit("reset", props.user);
  } else if (confirmationAction.value === "revoke-sessions") {
    emit("revoke", props.user);
  }
  void closeConfirmation();
}

watch(() => props.user, resetDraft, { deep: true });
</script>

<template>
  <article class="rounded-xl border bg-[var(--color-surface-elevated)] p-4" :style="{ borderColor: 'var(--color-border-default)' }">
    <div class="flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between">
      <div class="min-w-0">
        <div class="flex flex-wrap items-center gap-2"><h3 class="truncate font-bold">{{ user.display_name }}</h3><span v-if="isSelf" class="rounded-full bg-[var(--color-surface-tertiary)] px-2 py-1 text-xs font-bold">You</span><span v-if="!user.is_active" class="rounded-full bg-[color-mix(in_oklch,var(--color-status-error)_15%,transparent)] px-2 py-1 text-xs font-bold">Inactive</span></div>
        <p class="mt-1 font-mono text-xs text-[var(--color-text-secondary)]">{{ user.username }}</p>
        <p class="mt-1 text-xs text-[var(--color-text-tertiary)]">Last sign-in: {{ user.last_login_at ? new Date(user.last_login_at).toLocaleString() : "Never" }}</p>
      </div>

      <div v-if="sessionsOnly" class="flex items-center gap-3">
        <p class="text-sm text-[var(--color-text-secondary)]">Revokes every browser session for this account.</p>
        <button :disabled="busy" class="min-h-11 shrink-0 rounded-lg border px-4 text-sm font-bold disabled:opacity-50" :style="{ borderColor: 'var(--color-border-default)' }" @click="openConfirmation('revoke-sessions', $event)">Revoke sessions</button>
      </div>

      <div v-else class="grid gap-3 sm:grid-cols-2 xl:grid-cols-[minmax(11rem,1fr)_10rem_auto_auto] xl:items-end">
        <label class="grid gap-1 text-xs font-bold uppercase tracking-wider text-[var(--color-text-secondary)]">Display name<input v-model="displayName" class="min-h-11 rounded-lg border bg-[var(--color-surface-primary)] px-3 text-sm font-normal normal-case tracking-normal text-[var(--color-text-primary)]" :style="{ borderColor: 'var(--color-border-default)' }" /></label>
        <label class="grid gap-1 text-xs font-bold uppercase tracking-wider text-[var(--color-text-secondary)]">Access<select v-model="role" :disabled="finalActiveAdministrator" class="min-h-11 rounded-lg border bg-[var(--color-surface-primary)] px-3 text-sm font-normal normal-case tracking-normal text-[var(--color-text-primary)] disabled:opacity-50" :style="{ borderColor: 'var(--color-border-default)' }"><option value="requester">Requester</option><option value="technician">Technician</option><option value="administrator">Administrator</option></select></label>
        <label class="flex min-h-11 items-center gap-2 self-end rounded-lg border px-3 text-sm font-semibold" :style="{ borderColor: 'var(--color-border-default)' }"><input v-model="isActive" type="checkbox" :disabled="finalActiveAdministrator" /> Active</label>
        <div class="flex flex-wrap gap-2 self-end sm:col-span-2 xl:col-span-1">
          <button :disabled="busy || !dirty" class="min-h-11 rounded-lg bg-[var(--color-accent-primary)] px-4 text-sm font-bold text-white disabled:opacity-40" @click="requestSave">Save</button>
          <button :disabled="busy || isSelf" :title="isSelf ? 'Change your own password from Settings.' : undefined" class="min-h-11 rounded-lg border px-4 text-sm font-bold disabled:opacity-40" :style="{ borderColor: 'var(--color-border-default)' }" @click="openConfirmation('reset-password', $event)">Reset password</button>
        </div>
      </div>
    </div>
    <p v-if="finalActiveAdministrator && !sessionsOnly" class="mt-3 text-sm text-[var(--color-status-warning)]">Create or activate another administrator before changing this account’s access.</p>

    <dialog ref="confirmationDialog" class="m-auto w-[min(32rem,calc(100%-2rem))] rounded-2xl border bg-[var(--color-surface-elevated)] p-0 text-[var(--color-text-primary)] shadow-[var(--shadow-lg)] backdrop:bg-black/60" @cancel.prevent="closeConfirmation">
      <form method="dialog" class="p-6" @submit.prevent="confirmAction">
        <h2 class="text-xl font-bold">Confirm security action</h2>
        <p class="mt-3 text-sm text-[var(--color-text-secondary)]"><template v-if="confirmationAction === 'reset-password'">Resetting {{ user.display_name }}’s password revokes their sessions and creates a new temporary password.</template><template v-else-if="confirmationAction === 'revoke-sessions'">Every active session for {{ user.display_name }} will be signed out immediately.</template><template v-else>Your current session will be rotated. Confirm this privilege change with your current password.</template></p>
        <label v-if="confirmationAction === 'self-update'" class="mt-5 grid gap-2 text-sm font-semibold">Current password<input v-model="currentPassword" type="password" required autocomplete="current-password" class="min-h-11 rounded-lg border bg-[var(--color-surface-primary)] px-3" :style="{ borderColor: 'var(--color-border-default)' }" /></label>
        <div class="mt-6 flex justify-end gap-2"><button type="button" class="min-h-11 rounded-lg border px-4 text-sm font-bold" :style="{ borderColor: 'var(--color-border-default)' }" @click="closeConfirmation">Cancel</button><button type="submit" class="min-h-11 rounded-lg bg-[var(--color-status-error)] px-4 text-sm font-bold text-white">Confirm</button></div>
      </form>
    </dialog>
  </article>
</template>

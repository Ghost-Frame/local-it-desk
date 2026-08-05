<script setup lang="ts">
/** Ephemeral one-time credential display with copy and print actions. */
import { ref, watchEffect } from "vue";
import QRCode from "qrcode";

import { credentialsText } from "@/lib/account-admin";
import { loginQrPayload } from "@/lib/staff-onboarding";
import type { OneTimeCredential } from "@/types/api";

const props = defineProps<{
  /** Credentials held only by the current component instance. */
  credentials: OneTimeCredential[];
  /** Same-origin desk address printed on every card. */
  deskUrl: string;
}>();

const emit = defineEmits<{
  /** Clears credential material from parent memory. */
  dismiss: [];
}>();

/** Brief copy-operation status for assistive feedback. */
const copyStatus = ref("");
/** Data URL for a locally generated SVG containing only the desk address. */
const qrCodeSource = ref("");

watchEffect(async () => {
  try {
    const svg = await QRCode.toString(loginQrPayload(props.deskUrl), {
      type: "svg",
      errorCorrectionLevel: "M",
      margin: 1,
      width: 132,
    });
    qrCodeSource.value = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  } catch {
    qrCodeSource.value = "";
  }
});

/** Copies the visible onboarding sheet without persisting it. */
async function copyCredentials(): Promise<void> {
  await navigator.clipboard.writeText(credentialsText(props.credentials, props.deskUrl));
  copyStatus.value = "Copied. Clear the clipboard after delivery.";
}

/** Opens the browser print workflow for the visible onboarding sheet. */
function printCredentials(): void {
  window.print();
}
</script>

<template>
  <aside class="onboarding-sheet rounded-2xl border-2 border-[var(--color-status-warning)] bg-[var(--color-surface-elevated)] p-5 shadow-[var(--shadow-md)]" aria-labelledby="onboarding-title">
    <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
      <div>
        <p class="font-mono text-xs uppercase tracking-[0.18em] text-[var(--color-status-warning)]">Shown once</p>
        <h2 id="onboarding-title" class="mt-2 text-xl font-bold">Deliver these temporary passwords now</h2>
        <p class="mt-1 max-w-2xl text-sm text-[var(--color-text-secondary)]">They cannot be recovered after this panel is dismissed. Each person must change their password at first sign-in.</p>
      </div>
      <div class="flex flex-wrap gap-2 print:hidden">
        <button class="min-h-11 rounded-lg border px-4 text-sm font-bold" :style="{ borderColor: 'var(--color-border-default)' }" @click="copyCredentials">Copy all</button>
        <button class="min-h-11 rounded-lg border px-4 text-sm font-bold" :style="{ borderColor: 'var(--color-border-default)' }" @click="printCredentials">Print</button>
        <button class="min-h-11 rounded-lg bg-[var(--color-text-primary)] px-4 text-sm font-bold text-[var(--color-text-inverse)]" @click="emit('dismiss')">I saved them</button>
      </div>
    </div>
    <p class="mt-3 text-sm" role="status">{{ copyStatus }}</p>
    <div class="mt-4 grid gap-3 md:grid-cols-2 print:grid-cols-2">
      <article v-for="entry in credentials" :key="entry.user.id" class="login-card break-inside-avoid rounded-xl border bg-white p-5 text-black" :style="{ borderColor: 'var(--color-border-strong)' }">
        <div class="flex items-start justify-between gap-4">
          <div><p class="font-mono text-[0.68rem] font-bold uppercase tracking-[0.18em]">Staff login card</p><p class="mt-2 text-lg font-bold">{{ entry.user.display_name }}</p></div>
          <img v-if="qrCodeSource" :src="qrCodeSource" alt="QR code for the desk address only" class="h-24 w-24 shrink-0" />
        </div>
        <p class="mt-4 break-all text-sm"><strong>Desk:</strong> {{ deskUrl }}</p>
        <dl class="mt-3 grid gap-1 text-sm">
          <dt class="font-semibold">Username</dt><dd class="font-mono">{{ entry.user.username }}</dd>
          <dt class="mt-2 font-semibold">Temporary password</dt><dd class="break-all font-mono text-base font-bold">{{ entry.temporary_password }}</dd>
        </dl>
        <p class="mt-4 border-t border-black/20 pt-3 text-xs leading-5">Sign in, then create a new private password when asked. Contact the local technician if this card is lost before first use.</p>
      </article>
    </div>
  </aside>
</template>

<style scoped>
/** Keeps only the credential sheet visible in browser print output. */
@media print {
  :global(body *) {
    visibility: hidden;
  }
  .onboarding-sheet {
    position: fixed;
    inset: 0;
    z-index: 9999;
    border: 0;
    background: white;
    color: black;
    visibility: visible;
  }
  .onboarding-sheet * {
    visibility: visible;
  }
  .login-card {
    break-inside: avoid;
    box-shadow: none;
  }
}
</style>

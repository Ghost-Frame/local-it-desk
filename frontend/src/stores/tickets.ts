/** Minimal ticket store boundary reserved for the ticket implementation plan. */

import { ref } from "vue";
import { defineStore } from "pinia";

import type { Ticket } from "@/types";

/** Holds the currently visible tickets without implementing persistence operations. */
export const useTicketsStore = defineStore("tickets", () => {
  const tickets = ref<Ticket[]>([]);
  const isLoading = ref(false);
  const error = ref<string | null>(null);
  return { tickets, isLoading, error };
});

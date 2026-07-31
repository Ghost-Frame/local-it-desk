import { ref, watchEffect } from "vue";
import { usePreferredDark, useLocalStorage } from "@vueuse/core";

/** Browser theme preference stored between visits. */
type ThemeMode = "light" | "dark" | "system";

/** Composable for managing light/dark theme state. */
export function useTheme() {
  const stored = useLocalStorage<ThemeMode>("it-desk-theme", "system");
  const prefersDark = usePreferredDark();
  const isDark = ref(false);

  watchEffect(() => {
    isDark.value =
      stored.value === "dark" ||
      (stored.value === "system" && prefersDark.value);

    if (isDark.value) {
      document.documentElement.classList.add("dark");
    } else {
      document.documentElement.classList.remove("dark");
    }
  });

  /** Set the theme mode (light, dark, or system). */
  function setTheme(mode: ThemeMode) {
    stored.value = mode;
  }

  return { isDark, theme: stored, setTheme };
}

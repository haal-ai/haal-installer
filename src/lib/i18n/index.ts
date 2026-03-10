import { register, init, getLocaleFromNavigator } from "svelte-i18n";

register("en", () => import("./en.json"));
register("fr", () => import("./fr.json"));

const navigatorLocale = getLocaleFromNavigator() ?? "en";
const initialLocale = navigatorLocale.startsWith("fr") ? "fr" : "en";

init({
  fallbackLocale: "en",
  initialLocale,
});

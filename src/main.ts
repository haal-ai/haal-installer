import { waitLocale } from "svelte-i18n";
import "./lib/i18n";
import App from "./App.svelte";
import { mount } from "svelte";
import "./app.css";

waitLocale().then(() => {
  mount(App, {
    target: document.getElementById("app")!,
  });
});

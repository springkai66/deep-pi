import { mockIPC } from "@tauri-apps/api/mocks";
import "../../src/app.css";
import { mount } from "svelte";
import SettingsFixture from "./SettingsFixture.svelte";

mockIPC((command) => {
  if (command === "list_system_fonts") return ["Fixture Sans", "Fixture Mono"];
  if (command === "plugin:app|version") return "1.0.0";
  return null;
});
mount(SettingsFixture, { target: document.getElementById("app")! });

if (new URLSearchParams(location.search).has("smoke")) {
  void import("./settings-layout-smoke");
}

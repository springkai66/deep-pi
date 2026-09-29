import "../../src/app.css";
import { mount } from "svelte";
import SettingsFixture from "./SettingsFixture.svelte";

mount(SettingsFixture, { target: document.getElementById("app")! });

if (new URLSearchParams(location.search).has("smoke")) {
  void import("./settings-layout-smoke");
}

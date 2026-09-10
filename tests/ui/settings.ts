import "../../src/app.css";
import { mount } from "svelte";
import SettingsFixture from "./SettingsFixture.svelte";

mount(SettingsFixture, { target: document.getElementById("app")! });

import { mount } from "svelte";
import TaskTabsFixture from "./TaskTabsFixture.svelte";

export default mount(TaskTabsFixture, { target: document.getElementById("app")! });

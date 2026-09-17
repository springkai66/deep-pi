import { mount } from "svelte";
import RailFixture from "./RailFixture.svelte";

export default mount(RailFixture, { target: document.getElementById("app")! });

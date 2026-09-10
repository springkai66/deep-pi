import "../../src/app.css";
import { mount } from "svelte";
import GitPushFixture from "./GitPushFixture.svelte";

mount(GitPushFixture, { target: document.getElementById("app")! });

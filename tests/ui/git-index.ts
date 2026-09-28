import "../../src/app.css";
import { mount } from "svelte";
import GitIndexFixture from "./GitIndexFixture.svelte";

mount(GitIndexFixture, { target: document.getElementById("app")! });

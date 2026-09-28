import "../../src/app.css";
import { mount } from "svelte";
import GitCommitFixture from "./GitCommitFixture.svelte";

mount(GitCommitFixture, { target: document.getElementById("app")! });

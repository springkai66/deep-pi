import { mount } from "svelte";
import "../../src/app.css";
import GitDiffFixture from "./GitDiffFixture.svelte";

if (!import.meta.env.DEV) throw new Error("This component fixture is development-only");
mount(GitDiffFixture, { target: document.getElementById("app")! });

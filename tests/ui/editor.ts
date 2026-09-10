import { mount } from "svelte";
import "../../src/app.css";
import EditorFixture from "./EditorFixture.svelte";
mount(EditorFixture, { target: document.getElementById("app")! });

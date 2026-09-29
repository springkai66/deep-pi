type Category = "extensions" | "mcp" | "skills" | "workflows";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

async function waitFor<T>(read: () => T | null | undefined, label: string): Promise<T> {
  const deadline = Date.now() + 5000;
  while (Date.now() < deadline) {
    const value = read();
    if (value) return value;
    await new Promise((resolve) => setTimeout(resolve, 30));
  }
  throw new Error(`Timed out: ${label}`);
}

const panel = () => document.querySelector<HTMLElement>(".settings-panel:not([hidden])");
const button = (root: Element, text: string) => [...root.querySelectorAll<HTMLButtonElement>("button")].find((item) => item.textContent?.trim() === text);
const overflow = (root: HTMLElement, label: string) => {
  assert(root.scrollWidth <= root.clientWidth + 2, `${label}: horizontal overflow ${root.scrollWidth} > ${root.clientWidth}`);
  for (const item of root.querySelectorAll<HTMLElement>(".page-controls button, .group-header button, .market-toolbar button, .market-toolbar select, .entry-list > li > button, .market-row > button")) {
    const bounds = item.getBoundingClientRect();
    const limits = root.getBoundingClientRect();
    assert(bounds.left >= limits.left - 2 && bounds.right <= limits.right + 2, `${label}: clipped ${item.getAttribute("aria-label") || item.textContent}`);
  }
};

async function open(category: Category) {
  const nav = document.getElementById(`settings-nav-${category}`);
  assert(nav, `${category}: navigation entry`);
  nav.click();
  await new Promise((resolve) => setTimeout(resolve, 0));
  return waitFor(() => nav.getAttribute("aria-current") === "page" ? panel()?.querySelector<HTMLElement>(".page-controls")?.closest<HTMLElement>(".settings-panel") : null, `${category} panel`);
}

async function run() {
  const results: string[] = [];
  const projectAvailable = new URLSearchParams(location.search).has("project");
  for (const category of ["extensions", "mcp", "skills", "workflows"] as const) {
    const root = await open(category);
    const controls = root.querySelector<HTMLElement>(".page-controls");
    assert(controls, `${category}: controls`);
    const groups = controls.querySelectorAll<HTMLElement>(".scope-switch");
    assert(groups.length === 2, `${category}: scope and views`);
    assert([...groups[0].querySelectorAll("button")].map((item) => item.textContent?.trim()).join(",") === "全局,项目", `${category}: scope order`);
    assert([...groups[1].querySelectorAll("button")].map((item) => item.textContent?.trim()).join(",") === "已安装,市场", `${category}: view order`);
    const project = groups[0].querySelectorAll<HTMLButtonElement>("button")[1];
    assert(project.disabled === !projectAvailable, `${category}: project scope availability`);
    if (projectAvailable) {
      project.click();
      await waitFor(() => project.getAttribute("aria-pressed") === "true" && document.querySelector('output[aria-label="读取范围"]')?.textContent === "project", `${category}: project refresh scope`);
      groups[0].querySelector<HTMLButtonElement>("button")?.click();
      await waitFor(() => document.querySelector('output[aria-label="读取范围"]')?.textContent === "global", `${category}: global refresh scope`);
    }
    await waitFor(() => root.querySelector(".entry-list > li"), `${category} installed entries`);
    assert(root.querySelector(".group-header .icon-action"), `${category}: refresh installed`);
    overflow(root, `${category} installed`);
    button(groups[1], "市场")?.click();
    const search = await waitFor(() => root.querySelector<HTMLInputElement>(".market-toolbar input"), `${category} search`);
    assert(search && root.querySelector(".market-toolbar select"), `${category}: search and sort`);
    await waitFor(() => root.querySelector(".market-list > li"), `${category} market entries`);
    overflow(root, `${category} market`);
    results.push(`${category}: layout ok`);
    if (category === "extensions") {
      const install = [...root.querySelectorAll<HTMLLIElement>(".market-list > li")].find((item) => item.textContent?.includes("pi-other"));
      assert(install, "extension catalog row");
      button(install, "安装")?.click();
      await waitFor(() => document.querySelector('output[aria-label="操作结果"]')?.textContent === "install", "extension install");
      button(groups[1], "已安装")?.click();
      const update = await waitFor(() => root.querySelector<HTMLButtonElement>('[aria-label="更新 npm:pi-other@1.0.0"]'), "extension update button");
      update.click();
      await waitFor(() => document.querySelector('output[aria-label="操作结果"]')?.textContent === "update", "extension update");
      const remove = await waitFor(() => root.querySelector<HTMLButtonElement>('[aria-label="卸载 npm:pi-other@1.0.0"]:not(:disabled)'), "extension uninstall button");
      remove.click();
      await waitFor(() => document.querySelector('output[aria-label="操作结果"]')?.textContent === "remove", "extension uninstall");
      await waitFor(() => ![...root.querySelectorAll(".entry-list > li strong")].some((item) => item.textContent === "npm:pi-other@1.0.0"), "extension removed from installed");
      assert(root.querySelector('[role="status"]')?.textContent?.includes("已卸载"), "extension uninstall feedback");
      results.push("extension install, update, uninstall and feedback ok");
    }
    if (category === "mcp") {
      search.value = "GitHub";
      search.dispatchEvent(new Event("input", { bubbles: true }));
      await waitFor(() => root.querySelectorAll(".market-list > li").length === 1, "MCP filter");
      assert(root.querySelector(".market-list")?.textContent?.includes("GitHub"), "MCP filtered entry");
      button(groups[1], "已安装")?.click();
      const add = await waitFor(() => root.querySelector<HTMLInputElement>(".add-form input"), "MCP add form");
      const config = root.querySelector<HTMLTextAreaElement>(".add-form textarea");
      assert(config, "MCP config form");
      add.value = "fixture-added"; add.dispatchEvent(new Event("input", { bubbles: true }));
      config.value = '{"url":"https://example.com/new"}'; config.dispatchEvent(new Event("input", { bubbles: true }));
      root.querySelector<HTMLButtonElement>(".add-form button")?.click();
      await waitFor(() => root.textContent?.includes("fixture-added"), "MCP add visible");
      results.push("MCP filter and manual add ok");
    }
    if (category === "skills") {
      button(groups[1], "已安装")?.click();
      const inputs = await waitFor(() => {
        const fields = root.querySelectorAll<HTMLInputElement>(".add-form input");
        return fields.length === 2 ? fields : null;
      }, "Skills manual add form");
      const content = root.querySelector<HTMLTextAreaElement>(".add-form textarea");
      assert(content, "Skills manual add content");
      for (const [input, value] of [[inputs[0], "fixture-skill"], [inputs[1], "Fixture description"]] as const) {
        input.value = value;
        input.dispatchEvent(new Event("input", { bubbles: true }));
      }
      content.value = "# Fixture skill";
      content.dispatchEvent(new Event("input", { bubbles: true }));
      root.querySelector<HTMLButtonElement>(".add-form button")?.click();
      await waitFor(() => root.querySelector(".entry-list")?.textContent?.includes("fixture-skill"), "Skills manual add visible");
      results.push("Skills manual add ok");
    }
    if (category === "workflows") {
      const detail = root.querySelector<HTMLButtonElement>(".market-list .secondary-action");
      assert(detail, "workflow detail control");
      detail.click();
      await waitFor(() => root.querySelector(".market-detail .step-list"), "workflow steps");
      assert(root.querySelector(".market-detail .component-list") && root.querySelector(".market-detail .prompt-block"), "workflow components and prompt");
      overflow(root, "workflow expanded detail");
      const install = root.querySelector<HTMLButtonElement>(".market-list .market-row .primary-action");
      assert(install, "workflow install button");
      install.click();
      await waitFor(() => root.querySelector(".install-summary"), "workflow install feedback");
      results.push("workflow detail and install feedback ok");
    }
  }
  return results;
}

run().then((results) => {
  document.body.dataset.layoutSmoke = "pass";
  document.body.insertAdjacentHTML("beforeend", `<pre id="layout-smoke-result">PASS ${innerWidth}x${innerHeight}: ${results.join("; ")}</pre>`);
}).catch((error) => {
  document.body.dataset.layoutSmoke = "fail";
  document.body.insertAdjacentHTML("beforeend", `<pre id="layout-smoke-result">FAIL ${String(error)}</pre>`);
});

export {};

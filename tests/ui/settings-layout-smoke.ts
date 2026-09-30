type Category = "extensions" | "mcp" | "skills" | "workflows";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

async function waitFor<T>(read: () => T | null | undefined, label: string): Promise<T> {
  const deadline = Date.now() + 10000;
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
  return waitFor(() => {
    if (nav.getAttribute("aria-current") !== "page") return null;
    const controls = panel()?.querySelector<HTMLElement>(".page-controls");
    const root = controls?.closest<HTMLElement>(".settings-panel");
    const body = document.querySelector<HTMLElement>(".settings-body");
    return root && body && root.getBoundingClientRect().bottom <= body.getBoundingClientRect().bottom + 2 ? root : null;
  }, `${category} panel`);
}

async function run() {
  const results: string[] = [];
  const search = document.querySelector<HTMLInputElement>('input[aria-label="搜索设置"]');
  assert(search, "settings search exists");
  search.value = "代理地址";
  search.dispatchEvent(new Event("input", { bubbles: true }));
  await waitFor(() => document.querySelectorAll(".settings-navigation button[id^='settings-nav-']").length === 1, "settings filtered navigation");
  assert(document.getElementById("settings-nav-network"), "network matched by setting name");
  search.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  assert(document.activeElement?.id === "settings-nav-network", "search ArrowDown focuses the first matching category");
  window.scrollTo(0, 0);
  search.value = "不存在的设置";
  search.dispatchEvent(new Event("input", { bubbles: true }));
  await waitFor(() => document.querySelector(".settings-no-results"), "empty search feedback");
  search.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await waitFor(() => document.querySelectorAll(".settings-navigation button[id^='settings-nav-']").length === 11, "search cleared by Escape");
  const general = document.querySelector<HTMLElement>(".settings-panel:not([hidden])");
  const check = general?.querySelector<HTMLElement>(".setting-check");
  const checkbox = check?.querySelector<HTMLInputElement>('input[type="checkbox"]');
  const label = check?.querySelector<HTMLElement>("strong");
  assert(check && checkbox && label, "notification setting row");
  assert(label.getBoundingClientRect().left - checkbox.getBoundingClientRect().right <= 20, "checkbox and label stay together");
  const note = check.nextElementSibling as HTMLElement;
  assert(note?.classList.contains("muted") && getComputedStyle(check).borderBottomStyle === "none" && getComputedStyle(note).borderBottomStyle !== "none", "description belongs to its row");
  results.push("search and setting row layout ok");
  const projectAvailable = new URLSearchParams(location.search).has("project");
  for (const category of ["extensions", "mcp", "skills", "workflows"] as const) {
    const root = await open(category);
    const controls = root.querySelector<HTMLElement>(".page-controls");
    assert(controls, `${category}: controls`);
    const groups = controls.querySelectorAll<HTMLElement>(".scope-switch");
    const body = document.querySelector<HTMLElement>(".settings-body");
    assert(body, "settings body exists");
    assert(root.getBoundingClientRect().bottom <= body.getBoundingClientRect().bottom + 2, `${category}: panel fits available height`);
    assert(root.clientHeight > 0 && root.clientHeight < 2000, `${category}: panel height is constrained`);
    assert(root.getAttribute("aria-labelledby") === "settings-panel-title", `${category}: panel title semantics`);
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
      button(groups[1], "已安装")?.click();
      await waitFor(() => root.querySelector(".entry-list > li"), "MCP config loaded before connection check");
      const unrequestedStatus = await waitFor(() => {
        const status = root.querySelector<HTMLElement>(".mcp-native-status");
        return status?.textContent?.includes("尚未检测") ? status : null;
      }, "connection check is opt-in");
      assert(unrequestedStatus.textContent?.includes("尚未检测"), "opening MCP settings does not start configured servers");
      button(root, "检测连接")?.click();
      const codemodeToggle = await waitFor(() => root.querySelector<HTMLInputElement>(".codemode-toggle input"), "Codemode toggle");
      const nativeStatus = await waitFor(() => {
        const status = root.querySelector<HTMLElement>(".mcp-native-status");
        return status?.textContent?.includes("已连接") ? status : null;
      }, "native Pi MCP connection status");
      assert(nativeStatus.textContent?.includes("已连接"), "MCP state comes from Pi's native list command");
      codemodeToggle.checked = true;
      codemodeToggle.dispatchEvent(new Event("change", { bubbles: true }));
      button(root, "保存 Codemode 设置")?.click();
      await waitFor(() => document.querySelector('output[aria-label="操作结果"]')?.textContent === "save_pi_codemode_settings", "Codemode settings saved through Pi settings bridge");
      button(groups[1], "市场")?.click();
      const mcpSearch = await waitFor(() => root.querySelector<HTMLInputElement>(".market-toolbar input"), "MCP search");
      mcpSearch.value = "GitHub";
      mcpSearch.dispatchEvent(new Event("input", { bubbles: true }));
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
      await waitFor(() => [...root.querySelectorAll(".entry-list > li")].some((item) => item.textContent?.includes("fixture-added")), "MCP config is visible after adding");
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

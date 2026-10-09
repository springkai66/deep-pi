type Category = "pi" | "extensions" | "mcp" | "skills" | "workflows";

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
    const root = panel();
    if (!root?.querySelector(category === "pi" ? ".codemode-settings" : ".page-controls")) return null;
    const body = document.querySelector<HTMLElement>(".settings-body");
    return body && (category === "pi" || root.getBoundingClientRect().bottom <= body.getBoundingClientRect().bottom + 2) ? root : null;
  }, `${category} panel`);
}

async function checkFontDropdowns() {
  const nav = document.getElementById("settings-nav-appearance");
  assert(nav, "appearance navigation entry");
  nav.click();
  await waitFor(() => nav.getAttribute("aria-current") === "page" ? nav : null, "appearance panel");
  const triggers = [...document.querySelectorAll<HTMLButtonElement>(".font-trigger")];
  assert(triggers.length === 3, "all three font controls are rendered");

  for (const [index, trigger] of triggers.entries()) {
    const label = trigger.getAttribute("aria-label") ?? `font ${index + 1}`;
    assert(trigger.getBoundingClientRect().width > 0, `${label}: trigger is visible`);
    trigger.scrollIntoView({ block: "center" });
    await new Promise((resolve) => setTimeout(resolve, 0));
    if (index === 2) {
      trigger.focus();
      trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    } else {
      trigger.click();
    }
    const list = await waitFor(() => {
      const id = trigger.getAttribute("aria-controls");
      return id ? document.getElementById(id) : null;
    }, `${label}: options open`);
    assert(trigger.getAttribute("aria-expanded") === "true", `${label}: expanded state`);
    assert(list.parentElement === document.body, `${label}: menu escapes settings clipping`);
    const bounds = list.getBoundingClientRect();
    assert(bounds.top >= 0 && bounds.bottom <= innerHeight && bounds.left >= 0 && bounds.right <= innerWidth,
      `${label}: menu is fully inside the viewport`);
    assert(list.querySelectorAll('[role="option"]').length > 1, `${label}: font options are populated`);
    const fixtureFont = [...list.querySelectorAll<HTMLButtonElement>('[role="option"]')]
      .find((option) => option.textContent?.trim() === "Fixture Sans");
    assert(fixtureFont, `${label}: fixture font option exists`);
    fixtureFont.click();
    await waitFor(() => trigger.querySelector(".font-preview")?.textContent?.trim() === "Fixture Sans", `${label}: selection applied`);
    await waitFor(() => !document.getElementById(list.id), `${label}: menu closes after selection`);
  }
}

const output = (label: string) => document.querySelector(`output[aria-label="${label}"]`)?.textContent;
const packageNames = (root: HTMLElement) => [...root.querySelectorAll(".market-list > li strong")].map((item) => item.textContent).join(",");

async function checkCatalog(root: HTMLElement) {
  const field = root.querySelector<HTMLInputElement>('.market-search input[aria-label="搜索 Pi 资源包"]');
  const form = root.querySelector<HTMLFormElement>(".market-search");
  assert(field && form, "Pi 资源包 search uses package terminology");
  const submit = async (query: string) => {
    field.value = query;
    field.dispatchEvent(new Event("input", { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 0));
    form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await waitFor(() => output("目录请求")?.endsWith(`${query.trim()}:1`), `${query}: first catalog page requested`);
  };
  const expectAdvisor = async () => waitFor(() => packageNames(root) === "pi-advisor-flow,pi-advisor", "advisor official order and skill result");
  assert(output("目录请求") === ":1", "opening packages automatically requests the popular first page");
  assert(packageNames(root) === "pi-demo,pi-other", "popular packages retain server order");
  assert(!root.querySelector(".market-toolbar select"), "catalog has one official downloads order");
  await submit("advisor");
  await expectAdvisor();
  assert(!button(root, "加载更多"), "last page has no load-more control");

  await submit("fail-once");
  await waitFor(() => root.querySelector(".catalog-error"), "persistent first-page failure");
  assert(!root.textContent?.includes("没有匹配的 Package"), "failure is distinct from a true empty result");
  field.value = "unsent-draft";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  button(root, "重试")?.click();
  await waitFor(() => packageNames(root) === "pi-page-first,pi-page-theme", "retry failed first-page query");
  assert(output("目录请求")?.endsWith("fail-once:1,fail-once:1"), "retry preserves submitted query despite unsent input edits");
  assert(!root.querySelector(".catalog-error"), "successful retry clears persistent failure");

  await submit("page-fail");
  await waitFor(() => button(root, "加载更多"), "first page offers load-more");
  button(root, "加载更多")?.click();
  await waitFor(() => root.querySelector(".catalog-error"), "load-more failure");
  assert(packageNames(root) === "pi-page-first,pi-page-theme", "failed later page preserves loaded rows");
  button(root, "重试")?.click();
  await waitFor(() => root.querySelectorAll(".market-list > li").length === 4, "retry appends later page");
  assert(output("目录请求")?.endsWith("page-fail:2,page-fail:2"), "retry requests the failed later page");
  assert(packageNames(root) === "pi-page-first,pi-page-theme,pi-page-prompt,pi-page-untyped", "all types, untyped packages, deduplication and server order across pages");
  assert(!button(root, "加载更多"), "pagination ends after the last page");
  overflow(root, "Pi 资源包 all resource types");

  await submit("empty");
  await waitFor(() => root.textContent?.includes("没有匹配的 Package"), "true empty result");
  assert(!root.querySelector(".catalog-error") && !root.querySelector(".market-list"), "empty result has no error or stale packages");
  for (const query of ["slow", "slow-fail", "slow-page"]) {
    await submit(query);
    if (query === "slow-page") {
      await waitFor(() => button(root, "加载更多"), "slow page first batch loaded");
      button(root, "加载更多")?.click();
    }
    await waitFor(() => output("待完成目录请求") === "1", `${query}: old request pending`);
    await submit("advisor");
    await expectAdvisor();
    await waitFor(() => output("待完成目录请求") === "0", `${query}: old request finished`);
    assert(packageNames(root) === "pi-advisor-flow,pi-advisor" && !root.querySelector(".catalog-error"), `${query}: stale response cannot replace or append to new results`);
    assert(!document.querySelector('[aria-label="测试状态"]')?.textContent?.includes("Fixture stale catalog request failed"), "stale failures do not report errors");
  }
  await submit("   ");
  await waitFor(() => packageNames(root) === "pi-demo,pi-other", "cleared query restores popular first page");
  assert(button(root, "加载更多"), "cleared query restores first-page pagination");
}

async function checkCodemode(projectAvailable: boolean) {
  const params = new URLSearchParams(location.search);
  const root = await open("pi");
  const card = root.querySelector<HTMLElement>(".codemode-settings");
  const runtime = root.querySelector(".runtime-settings");
  assert(card && runtime, "Pi 服务 contains runtime management and independent Codemode settings");
  assert(runtime.compareDocumentPosition(card) & Node.DOCUMENT_POSITION_FOLLOWING, "Codemode follows runtime management");
  assert(document.getElementById("settings-panel-title")?.textContent === "Pi 服务", "Codemode belongs to Pi 服务");
  if (params.has("codemode-read-fails")) {
    await waitFor(() => card.querySelector('[role="alert"]')?.textContent?.includes("Fixture Codemode read failed"), "Codemode read failure");
    assert(!card.querySelector(".save-codemode"), "failed read cannot save defaults over existing settings");
    button(card, "重试")?.click();
  }
  const toggle = await waitFor(() => card.querySelector<HTMLInputElement>('.codemode-toggle input'), "independent Codemode read");
  const mode = card.querySelector<HTMLSelectElement>('select[aria-label="Codemode 模式"]');
  const budget = card.querySelector<HTMLInputElement>('input[aria-label="工具描述预算（tokens）"]');
  assert(mode && budget, "Codemode mode and inline budget");
  assert(toggle.checked === params.has("codemode-enabled"), "default disabled and explicit enabled configuration preserved");
  assert(mode.value === (params.has("codemode-enabled") ? "only" : "on") && budget.value === (params.has("codemode-enabled") ? "4200" : "3000"), "existing values or unconfigured mode and budget preserved");
  assert(card.textContent?.includes("MCP 仍可能按需自动启用 Codemode"), "default startup and MCP activation semantics explained");
  const project = button(card, "当前项目");
  assert(project && project.disabled === !projectAvailable, "Codemode project scope availability");
  const save = button(card, "保存 Codemode 设置");
  assert(save, "Codemode save control");
  save.scrollIntoView({ block: "center" });
  await new Promise((resolve) => setTimeout(resolve, 0));
  const body = document.querySelector<HTMLElement>(".settings-body");
  const bounds = save.getBoundingClientRect();
  assert(body && bounds.top >= body.getBoundingClientRect().top && bounds.bottom <= body.getBoundingClientRect().bottom,
    "Codemode save is reachable by scrolling the settings body");
  budget.value = "100001";
  budget.dispatchEvent(new Event("input", { bubbles: true }));
  button(card, "保存 Codemode 设置")?.click();
  await waitFor(() => card.querySelector('[role="alert"]')?.textContent?.includes("0 到 100000"), "invalid inline budget rejected");
  assert(output("Codemode 保存次数") === "0", "invalid settings never invoke save");
  toggle.checked = true;
  toggle.dispatchEvent(new Event("change", { bubbles: true }));
  mode.value = "only";
  mode.dispatchEvent(new Event("change", { bubbles: true }));
  budget.value = "4242";
  budget.dispatchEvent(new Event("input", { bubbles: true }));
  button(card, "保存 Codemode 设置")?.click();
  await waitFor(() => output("Codemode 忙碌") === "true", "Codemode save blocks settings close");
  assert(button(card, "全局")?.disabled && project.disabled, "scope cannot change while saving");
  assert(document.querySelector<HTMLButtonElement>('.settings-header [aria-label="返回工作区"]')?.disabled, "return is disabled while saving Codemode");
  if (params.has("codemode-save-fails")) {
    await waitFor(() => card.querySelector('[role="alert"]')?.textContent?.includes("Fixture Codemode save failed"), "Codemode save failure");
    await waitFor(() => output("Codemode 忙碌") === "false", "failed save releases busy state");
    assert(card.querySelector<HTMLInputElement>(".codemode-toggle input")?.checked, "failed save preserves user edits");
    button(card, "保存 Codemode 设置")?.click();
  }
  await waitFor(() => output("Codemode 保存次数") === "1" && output("Codemode 忙碌") === "false", "global Codemode save completes");
  await waitFor(() => card.querySelector(".codemode-notice")?.textContent?.includes("已运行的 Pi 任务重启后生效"), "save restart notice");
  assert(output("Codemode 保存范围") === "global", "global Codemode saved through existing command");
  if (projectAvailable) {
    project.click();
    const projectToggle = await waitFor(() => output("Codemode 读取范围") === "project" ? card.querySelector<HTMLInputElement>(".codemode-toggle input") : null, "project Codemode inheritance loaded");
    assert(projectToggle.checked && card.querySelector<HTMLSelectElement>("select")?.value === "only" && card.querySelector<HTMLInputElement>('input[type="number"]')?.value === "4242", "project inherits global values without project config");
    projectToggle.checked = false;
    projectToggle.dispatchEvent(new Event("change", { bubbles: true }));
    button(card, "保存 Codemode 设置")?.click();
    await waitFor(() => output("Codemode 保存次数") === "2" && output("Codemode 忙碌") === "false", "project Codemode save completes");
    assert(output("Codemode 保存范围") === "project", "project save preserves projectPath");
    button(card, "全局")?.click();
    const globalToggle = await waitFor(() => output("Codemode 读取范围") === "global" ? card.querySelector<HTMLInputElement>(".codemode-toggle input") : null, "global Codemode reload");
    assert(globalToggle.checked, "project override does not alter global enabled setting");
    project.click();
    const savedProjectToggle = await waitFor(() => output("Codemode 读取范围") === "project" ? card.querySelector<HTMLInputElement>(".codemode-toggle input") : null, "saved project Codemode reload");
    assert(!savedProjectToggle.checked, "explicit project override remains respected");
  }
  overflow(root, "Pi 服务 Codemode");
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
  await checkFontDropdowns();
  results.push("all font dropdowns open and select");
  for (const [term, category] of [["Pi 资源包", "extensions"], ["Codemode", "pi"], ["内联预算", "pi"]] as const) {
    search.value = term;
    search.dispatchEvent(new Event("input", { bubbles: true }));
    await waitFor(() => document.querySelectorAll(".settings-navigation button[id^='settings-nav-']").length === 1, `${term}: settings search match`);
    assert(document.getElementById(`settings-nav-${category}`), `${term}: correct settings category`);
    search.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await waitFor(() => document.querySelectorAll(".settings-navigation button[id^='settings-nav-']").length === 11, `${term}: settings search reset`);
  }
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
    const brokenMcp = category === "mcp" && new URLSearchParams(location.search).has("mcp-fails");
    if (brokenMcp) await waitFor(() => document.querySelector('[aria-label="测试状态"]')?.textContent?.includes("Fixture MCP config damaged"), "damaged MCP configuration reported");
    else await waitFor(() => root.querySelector(".entry-list > li"), `${category} installed entries`);
    if (category === "mcp") assert(!root.querySelector(".codemode-toggle"), "MCP no longer contains Codemode settings");
    assert(root.querySelector(".group-header .icon-action"), `${category}: refresh installed`);
    overflow(root, `${category} installed`);
    button(groups[1], "市场")?.click();
    const search = await waitFor(() => root.querySelector<HTMLInputElement>(".market-toolbar input"), `${category} search`);
    assert(search && (category === "extensions" || root.querySelector(".market-toolbar select")), `${category}: catalog search controls`);
    await waitFor(() => root.querySelector(".market-list > li"), `${category} market entries`);
    overflow(root, `${category} market`);
    results.push(`${category}: layout ok`);
    if (category === "extensions") {
      assert(document.getElementById("settings-panel-title")?.textContent === "Pi 资源包", "Pi 资源包 category title");
      await checkCatalog(root);
      results.push("package pagination, all types, retry, empty results and stale queries ok");
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
    if (category === "mcp" && !brokenMcp) {
      button(groups[1], "已安装")?.click();
      await waitFor(() => root.querySelector(".entry-list > li"), "MCP config loaded before connection check");
      const unrequestedStatus = await waitFor(() => {
        const status = root.querySelector<HTMLElement>(".mcp-native-status");
        return status?.textContent?.includes("尚未检测") ? status : null;
      }, "connection check is opt-in");
      assert(unrequestedStatus.textContent?.includes("尚未检测"), "opening MCP settings does not start configured servers");
      button(root, "检测连接")?.click();
      const nativeStatus = await waitFor(() => {
        const status = root.querySelector<HTMLElement>(".mcp-native-status");
        return status?.textContent?.includes("已连接") ? status : null;
      }, "native Pi MCP connection status");
      assert(nativeStatus.textContent?.includes("已连接"), "MCP state comes from Pi's native list command");
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
  await checkCodemode(projectAvailable);
  results.push("Pi 服务 Codemode defaults, inheritance, both save scopes and independent error handling ok");
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

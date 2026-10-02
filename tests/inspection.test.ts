import { beforeEach, expect, test, vi } from "vitest";

const transport = vi.hoisted(() => ({
  window: "overview",
  invoke: vi.fn<() => Promise<void>>(),
  setup: vi.fn<() => Promise<void>>(),
  render: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: transport.invoke }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ label: transport.window }),
}));
vi.mock("tauri-plugin-mcp", () => ({ setupPluginListeners: transport.setup }));
vi.mock("react-dom/client", () => ({
  createRoot: () => ({ render: transport.render }),
}));
vi.mock("../src/app/App", () => ({ App: () => null }));

beforeEach(() => {
  vi.resetModules();
  transport.window = "overview";
  transport.invoke.mockResolvedValue(undefined);
  transport.setup.mockResolvedValue(undefined);
  document.body.innerHTML = '<div id="root"></div>';
});

async function launch(): Promise<void> {
  await import("../src/main");
  await vi.dynamicImportSettled();
  expect(transport.render).toHaveBeenCalledOnce();
}

test("settings registers no guest handlers even if a plugin permission is granted", async () => {
  transport.window = "settings";
  await launch();
  expect(transport.invoke).not.toHaveBeenCalled();
  expect(transport.setup).not.toHaveBeenCalled();
});

test("ordinary development without the inspection grant registers no guest handlers", async () => {
  transport.invoke.mockRejectedValue(new Error("mcp:push_ipc is not allowed"));
  await launch();
  expect(transport.invoke).toHaveBeenCalledWith("plugin:mcp|push_ipc");
  expect(transport.setup).not.toHaveBeenCalled();
});

test("authorized overview registers guest handlers after the host permission check", async () => {
  const permission = Promise.withResolvers<undefined>();
  transport.invoke.mockReturnValue(permission.promise);
  await import("../src/main");
  await vi.waitFor(() => {
    expect(transport.invoke).toHaveBeenCalledOnce();
  });
  expect(transport.setup).not.toHaveBeenCalled();
  permission.resolve(undefined);
  await vi.dynamicImportSettled();
  expect(transport.setup).toHaveBeenCalledOnce();
});

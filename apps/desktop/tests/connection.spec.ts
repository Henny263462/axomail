import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { invoke } from "@tauri-apps/api/core";

import { testConnection } from "../src/api/connection";
import type { ConnectionTestResult, CreateAccountInput } from "../src/types/mail";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);

interface TauriGlobals {
  __TAURI_INTERNALS__?: unknown;
}

const input: CreateAccountInput = {
  displayName: "Klaus Ferber",
  email: "klaus@theferbers.eu",
  incoming: {
    protocol: "imap",
    host: "imap.theferbers.eu",
    port: 993,
    security: "tls",
    username: "klaus",
    password: "incoming-secret",
  },
  outgoing: {
    host: "smtp.theferbers.eu",
    port: 465,
    security: "tls",
    username: "klaus",
    password: "outgoing-secret",
  },
};

beforeEach(() => {
  vi.clearAllMocks();
  // Simulate the desktop shell so the safe wrapper actually invokes.
  (globalThis as TauriGlobals).__TAURI_INTERNALS__ = {};
});

afterEach(() => {
  delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
});

describe("testConnection", () => {
  it("invokes the test_connection command with the input payload", async () => {
    const result: ConnectionTestResult = {
      incomingOk: true,
      incomingError: null,
      outgoingOk: false,
      outgoingError: "Authentication failed.",
    };
    invokeMock.mockResolvedValue(result);

    const outcome = await testConnection(input);

    expect(invokeMock).toHaveBeenCalledWith("test_connection", { input });
    expect(outcome).toEqual(result);
  });

  it("resolves null in the browser preview", async () => {
    delete (globalThis as TauriGlobals).__TAURI_INTERNALS__;
    const outcome = await testConnection(input);
    expect(outcome).toBeNull();
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

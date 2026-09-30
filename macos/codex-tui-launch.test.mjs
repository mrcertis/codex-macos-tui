import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chmod, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { parseArguments } from "./codex-tui-launch.mjs";

const launcher = fileURLToPath(new URL("./codex-tui-launch.mjs", import.meta.url));
const prefix = ["--backend", "/mock/backend", "--frontend", "/mock/frontend", "--"];
const parse = (...args) => parseArguments([...prefix, ...args]);

test("preserves ordered and quoted MCP/hook config for both processes", () => {
  const args = ["-c", 'mcp_servers.browser.command="/path with spaces/server"', "-c", 'mcp_servers.browser.args=["token=a=b", "$(never execute)"]', "--config=hooks.enabled=false", "--no-daemon", "--model", "GPT-6-Sol", "--no-alt-screen"];
  const result = parse(...args);
  const config = args.slice(0, 4).concat("-c", "hooks.enabled=false");
  assert.deepEqual(result.backendConfig, config);
  assert.deepEqual(result.frontendArgs, [...config, "--model", "GPT-6-Sol", "--no-alt-screen"]);
});

test("matches stock auto/yolo/accept-edits semantics without changing defaults", () => {
  assert.deepEqual(parse().backendConfig, []);
  const original = ["-c", 'approval_policy="never"'];
  assert.deepEqual(parse(...original, "--approve-for-me").backendConfig, [...original, "-c", 'approvals_reviewer="auto_review"', "-c", 'approval_policy="on-request"', "-c", 'sandbox_mode="workspace-write"']);
  for (const flag of ["--dangerously-bypass-approvals-and-sandbox", "--yolo", "-yolo"]) {
    assert.deepEqual(parse(flag).backendConfig, ["-c", 'approval_policy="never"', "-c", 'sandbox_mode="danger-full-access"']);
    assert.deepEqual(parse(flag).frontendArgs, ["--dangerously-bypass-approvals-and-sandbox"]);
  }
  assert.deepEqual(parse("-s", "workspace-write", "-a", "on-request").backendConfig, ["-c", 'approval_policy="on-request"', "-c", 'sandbox_mode="workspace-write"']);
  for (const args of [["--approve-for-me", "--yolo"], ["--approve-for-me", "-a", "never"], ["--approve-for-me", "-s", "read-only"], ["--yolo", "-a", "never"], ["--approve-for-me", "--approve-for-me"]]) {
    assert.throws(() => parse(...args), /Conflicting|Duplicate/);
  }
});

test("resume cannot become an initial prompt and UUID is canonical", () => {
  const id = "123E4567-E89B-12D3-A456-426614174000";
  assert.deepEqual(parse("resume").frontendArgs, ["--resume"]);
  assert.deepEqual(parse("resume", id).frontendArgs, ["--resume-session-id", id.toLowerCase()]);
  assert.deepEqual(parse("resume", "--last", "--all", "--include-non-interactive").frontendArgs, ["--resume-last", "--resume-show-all", "--resume-include-non-interactive"]);
  for (const args of [["resume", "task text"], ["resume", id, "do a job"], ["resume", "123e4567e89b12d3a456426614174000"], ["resume", id, "--last"], ["do a job"], ["--", "do a job"], ["exec", "do a job"], ["--remote", "unix:///other.sock"]]) {
    assert.throws(() => parse(...args), /Resume|resume|Unsupported/);
  }
});

test("forwards native UI options while rejecting startup image messages", () => {
  const result = parse("-m", "model", "--profile=work", "--search", "--strict-config", "--oss", "--local-provider", "ollama", "--no-alt-screen");
  assert.deepEqual(result.frontendArgs, ["-m", "model", "--profile", "work", "--search", "--strict-config", "--oss", "--local-provider", "ollama", "--no-alt-screen"]);
  assert.deepEqual(result.backendConfig, []);
  for (const args of [["-i", "/image.png"], ["--image", "/image.png"], ["--image=/image.png"], ["-i=/image.png"], ["-i", "/image 1.png", "/image2.png"]]) {
    assert.throws(() => parse(...args), /Unsupported/);
  }
  for (const args of [["-m"], ["-a", "bad"], ["-s", "bad"], ["-c", "missing-equals"], ["--unknown-value", "potential prompt"], ["-m", "model", "prompt"]]) assert.throws(() => parse(...args));
  for (const argv of [[], ["--backend", "relative", "--frontend", "/front", "--"], ["--backend", "/back", "--backend", "/again", "--frontend", "/front", "--"]]) assert.throws(() => parseArguments(argv));
});

const mock = String.raw`
import { appendFileSync, readFileSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { createServer, createConnection } from 'node:net';
import { dirname } from 'node:path';
const args = process.argv.slice(2);
if (args[0] === '--version') {
  appendFileSync(process.env.MOCK_EVENTS, JSON.stringify({ type: 'version', pid: process.pid }) + '\n');
  if (process.env.MOCK_VERSION === 'wait') { setInterval(() => {}, 1000); await new Promise(() => {}); }
  console.log(process.env.MOCK_VERSION ?? 'codex-cli 0.159.2');
  process.exit(0);
}
const role = args[0] === 'app-server' ? 'backend' : process.env.MOCK_ROLE === 'descendant' ? 'descendant' : 'frontend';
const record = (type, data = {}) => appendFileSync(process.env.MOCK_EVENTS, JSON.stringify({ type, role, pid: process.pid, ...data }) + '\n');
record('start', { args, cwd: process.cwd(), codexHome: process.env.CODEX_HOME, disabled: process.env.CODEX_INTERNAL_APP_SERVER_REMOTE_CONTROL_DISABLED, sentinel: process.env.MOCK_SENTINEL });
if (role === 'descendant') {
  process.on('SIGTERM', () => record('term'));
  setInterval(() => {}, 1000);
} else if (role === 'backend') {
  if (process.env.MOCK_BACKEND === 'fail') process.exit(7);
  if (['descendant', 'crash-with-descendant'].includes(process.env.MOCK_BACKEND)) spawn(process.execPath, [process.argv[1]], { env: { ...process.env, MOCK_ROLE: 'descendant' }, stdio: process.env.MOCK_BACKEND === 'crash-with-descendant' ? ['ignore', 'inherit', 'inherit'] : 'ignore' });
  process.stdout.write('x'.repeat(180000)); process.stderr.write('y'.repeat(180000));
  const socket = args[args.indexOf('--listen') + 1].slice('unix://'.length);
  const server = createServer((connection) => { connection.on('error', () => {}); connection.end(); });
  if (process.env.MOCK_BACKEND !== 'no-listener') server.listen(socket, () => record('ready', { directory: dirname(socket) }));
  process.on('SIGTERM', () => {
    record('term');
    if (process.env.MOCK_BACKEND !== 'descendant') setTimeout(() => { record('exit'); process.exit(0); }, 120);
  });
  if (['crash-after-front', 'crash-with-descendant'].includes(process.env.MOCK_BACKEND)) {
    const timer = setInterval(() => {
      if (readFileSync(process.env.MOCK_EVENTS, 'utf8').includes('"type":"connected"')) { clearInterval(timer); setTimeout(() => process.exit(8), 100); }
    }, 20);
  }
  setInterval(() => {}, 1000);
} else {
  const socket = args[args.indexOf('--remote') + 1].slice('unix://'.length);
  const connection = createConnection(socket);
  connection.on('error', () => process.exit(9));
  connection.on('connect', () => { record('connected'); connection.destroy(); });
  process.on('SIGTERM', () => { record('term'); setTimeout(() => { record('exit'); process.exit(0); }, 120); });
  if (process.env.MOCK_FRONTEND !== 'wait') setTimeout(() => { record('exit'); process.exit(3); }, 250);
  else setInterval(() => {}, 1000);
}
`;

async function fixture(t, overrides = {}, codexArgs = []) {
  const directory = await mkdtemp(join(tmpdir(), "ctty-launch-test-"));
  const executable = join(directory, "mock.mjs");
  const events = join(directory, "events.jsonl");
  await writeFile(executable, `#!${process.execPath}\n${mock}`, { mode: 0o700 });
  await chmod(executable, 0o700);
  const child = spawn(process.execPath, [launcher, "--backend", executable, "--frontend", overrides.MOCK_FRONTEND_PATH ?? executable, "--", ...codexArgs], {
    cwd: directory, env: { ...process.env, CODEX_HOME: join(directory, "unchanged-home"), CODEX_INTERNAL_APP_SERVER_REMOTE_CONTROL_DISABLED: "original", MOCK_EVENTS: events, MOCK_SENTINEL: "same environment", ...overrides }, stdio: ["ignore", "pipe", "pipe"]
  });
  let output = "";
  child.stdout.on("data", (data) => { output += data; });
  child.stderr.on("data", (data) => { output += data; });
  const closed = new Promise((resolveClosed) => child.once("close", (code, signal) => resolveClosed({ code, signal })));
  let done = false;
  closed.then(() => { done = true; });
  const readEvents = async () => {
    try { return (await readFile(events, "utf8")).trim().split("\n").filter(Boolean).map((line) => JSON.parse(line)); }
    catch (error) { if (error.code === "ENOENT") return []; throw error; }
  };
  t.after(async () => {
    let timer;
    if (!done) {
      child.kill("SIGTERM");
      timer = setTimeout(() => { if (!done) child.kill("SIGKILL"); }, 5000);
    }
    try { await closed; } finally { clearTimeout(timer); }
    for (const event of await readEvents()) if (event.type === "start") {
      try { process.kill(event.pid, "SIGKILL"); } catch (error) { if (error.code !== "ESRCH") throw error; }
    }
    await rm(directory, { recursive: true, force: true });
  });
  return { child, closed, readEvents, output: () => output };
}

async function waitUntil(check, timeout = 5000) {
  const deadline = Date.now() + timeout;
  do { const result = await check(); if (result) return result; await delay(25); } while (Date.now() < deadline);
  assert.fail("mock process state did not arrive before timeout");
}

const exited = (pid) => { try { process.kill(pid, 0); return false; } catch (error) { if (error.code === "ESRCH") return true; throw error; } };
const missing = async (path) => { try { await stat(path); return false; } catch (error) { if (error.code === "ENOENT") return true; throw error; } };

test("real subprocesses get private socket, ordered settings, same cwd/env and bounded private logs", async (t) => {
  const input = ["--no-daemon", "-c", 'mcp_servers.browser.token="a=b"', "-c", "hooks.enabled=false", "resume", "123e4567-e89b-12d3-a456-426614174000"];
  const run = await fixture(t, { MOCK_FRONTEND: "wait" }, input);
  const ready = await waitUntil(async () => (await run.readEvents()).find((event) => event.type === "ready"));
  await waitUntil(async () => (await run.readEvents()).some((event) => event.role === "frontend" && event.type === "connected"));
  const events = await run.readEvents();
  const backend = events.find((event) => event.role === "backend" && event.type === "start");
  const frontend = events.find((event) => event.role === "frontend" && event.type === "start");
  assert.equal(backend.cwd, frontend.cwd);
  assert.equal(backend.codexHome, frontend.codexHome);
  assert.equal(backend.disabled, "1"); assert.equal(frontend.disabled, "original");
  assert.equal(backend.sentinel, frontend.sentinel);
  assert.deepEqual(backend.args.slice(3), input.slice(1, 5));
  assert.deepEqual(frontend.args.slice(2), ["--owned-local-server", ...input.slice(1, 5), "--resume-session-id", input[6]]);
  assert.equal((await stat(ready.directory)).mode & 0o777, 0o700);
  const log = await stat(join(ready.directory, "backend.log"));
  assert.equal(log.mode & 0o777, 0o600); assert.equal(log.size, 64 * 1024);
  assert.equal(run.output(), "");
  run.child.kill("SIGTERM");
  assert.equal((await run.closed).code, 143);
  assert.equal(await missing(ready.directory), true);
  assert.equal(exited(backend.pid), true); assert.equal(exited(frontend.pid), true);
  assert.ok((await run.readEvents()).some((event) => event.role === "backend" && event.type === "exit"));
});

test("frontend exit status is preserved and backend is reaped before cleanup", async (t) => {
  const run = await fixture(t);
  assert.equal((await run.closed).code, 3);
  const events = await run.readEvents();
  const backend = events.find((event) => event.role === "backend" && event.type === "start");
  const ready = events.find((event) => event.type === "ready");
  assert.ok(events.some((event) => event.role === "backend" && event.type === "exit"));
  assert.equal(exited(backend.pid), true); assert.equal(await missing(ready.directory), true);
});

test("backend startup failure never launches the frontend", async (t) => {
  const run = await fixture(t, { MOCK_BACKEND: "fail" });
  assert.equal((await run.closed).code, 1);
  const events = await run.readEvents();
  assert.equal(events.some((event) => event.role === "frontend"), false);
  const backend = events.find((event) => event.role === "backend");
  assert.equal(exited(backend.pid), true);
  assert.equal(await missing(backend.args[2].slice("unix://".length).replace(/\/rpc\.sock$/, "")), true);
});

test("backend version mismatch fails before starting a server or frontend", async (t) => {
  const run = await fixture(t, { MOCK_VERSION: "codex-cli 0.160.0" });
  assert.equal((await run.closed).code, 1);
  assert.deepEqual((await run.readEvents()).map((event) => event.type), ["version"]);
  assert.match(run.output(), /requires official codex-cli 0\.159\.2/);
  assert.equal(run.output().includes("0.160.0"), false);
});

test("SIGTERM during version check reaps that owned process without starting a server", async (t) => {
  const run = await fixture(t, { MOCK_VERSION: "wait" });
  const version = await waitUntil(async () => (await run.readEvents()).find((event) => event.type === "version"));
  run.child.kill("SIGTERM");
  assert.equal((await run.closed).code, 143);
  assert.equal(exited(version.pid), true);
  assert.deepEqual((await run.readEvents()).map((event) => event.type), ["version"]);
});

test("frontend spawn failure cleans the already listening backend", async (t) => {
  const run = await fixture(t, { MOCK_FRONTEND_PATH: "/nonexistent/canvastty-test-frontend" });
  assert.equal((await run.closed).code, 1);
  const events = await run.readEvents();
  assert.equal(exited(events.find((event) => event.role === "backend").pid), true);
  assert.equal(await missing(events.find((event) => event.type === "ready").directory), true);
});

test("backend failure while frontend is open terminates and reaps frontend", async (t) => {
  const run = await fixture(t, { MOCK_BACKEND: "crash-after-front", MOCK_FRONTEND: "wait" });
  assert.equal((await run.closed).code, 1);
  const events = await run.readEvents();
  assert.ok(events.some((event) => event.role === "frontend" && event.type === "term"));
  assert.equal(exited(events.find((event) => event.role === "frontend").pid), true);
  assert.equal(await missing(events.find((event) => event.type === "ready").directory), true);
});

test("backend exit triggers cleanup even when its descendant holds stdout/stderr pipes open", async (t) => {
  const run = await fixture(t, { MOCK_BACKEND: "crash-with-descendant", MOCK_FRONTEND: "wait" });
  assert.equal((await run.closed).code, 1);
  const events = await run.readEvents();
  const descendant = events.find((event) => event.role === "descendant" && event.type === "start");
  assert.ok(descendant);
  assert.ok(events.some((event) => event.role === "frontend" && event.type === "term"));
  await waitUntil(() => exited(descendant.pid));
  assert.equal(await missing(events.find((event) => event.type === "ready").directory), true);
});

test("SIGINT/SIGHUP before readiness cancel startup without launching jobs or frontend", async (t) => {
  for (const [signal, code] of [["SIGINT", 130], ["SIGHUP", 129]]) {
    const run = await fixture(t, { MOCK_BACKEND: "no-listener" });
    const backend = await waitUntil(async () => (await run.readEvents()).find((event) => event.role === "backend"));
    run.child.kill(signal);
    assert.equal((await run.closed).code, code);
    assert.equal((await run.readEvents()).some((event) => event.role === "frontend"), false);
    assert.equal(exited(backend.pid), true);
  }
});

test("bounded SIGKILL fallback reaps owned backend group including TERM-resistant descendant", async (t) => {
  const run = await fixture(t, { MOCK_BACKEND: "descendant", MOCK_FRONTEND: "wait" });
  const descendant = await waitUntil(async () => (await run.readEvents()).find((event) => event.role === "descendant" && event.type === "start"));
  const ready = await waitUntil(async () => (await run.readEvents()).find((event) => event.type === "ready"));
  await waitUntil(async () => (await run.readEvents()).some((event) => event.role === "frontend" && event.type === "connected"));
  const stoppedAt = performance.now();
  run.child.kill("SIGTERM");
  await waitUntil(async () => (await run.readEvents()).some((event) => event.role === "backend" && event.type === "term"));
  assert.equal(await missing(ready.directory), false);
  assert.equal((await run.closed).code, 143);
  assert.ok(performance.now() - stoppedAt < 2000, "cleanup must finish before CanvasTTY's 2s PTY deadline");
  await waitUntil(() => exited(descendant.pid));
  assert.equal(await missing(ready.directory), true);
  for (const event of await run.readEvents()) if (event.type === "start") assert.equal(exited(event.pid), true);
});

// Toolshed — MCP server aggregator
//
// Reads tool.json configs from TOOLS_DIR, spawns stdio-based MCP servers,
// wraps native tool scripts, and serves everything over HTTP JSON-RPC 2.0.

'use strict';

const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const { spawn, execFile } = require('node:child_process');
const { createInterface } = require('node:readline');

const PORT = parseInt(process.env.PORT || '8081', 10);
const TOOLS_DIR = process.env.TOOLS_DIR || '/tools';
const CALL_TIMEOUT_MS = 60_000;
const INIT_TIMEOUT_MS = 30_000;
const MAX_BODY_BYTES = 1024 * 1024; // 1 MB

// ---------------------------------------------------------------------------
// Environment variable resolution for tool.json ${VAR} placeholders
// ---------------------------------------------------------------------------

function resolveEnvValues(env) {
  const resolved = {};
  for (const [k, v] of Object.entries(env || {})) {
    resolved[k] = String(v).replace(/\$\{(\w+)\}/g, (_, name) => process.env[name] || '');
  }
  return resolved;
}

// ---------------------------------------------------------------------------
// Normalize commands that reference host-specific absolute paths
// ---------------------------------------------------------------------------

function normalizeCommand(toolName, command) {
  if (/^\/Users\/|^\/home\//.test(command)) {
    const fallback = path.join(TOOLS_DIR, toolName, 'run');
    if (fs.existsSync(fallback)) {
      console.log(`[${toolName}] remapped host path → ${fallback}`);
      return fallback;
    }
  }
  return command;
}

// ---------------------------------------------------------------------------
// MCP Stdio Client — manages a single stdio-transport MCP server process
// ---------------------------------------------------------------------------

class StdioMcpServer {
  constructor(name, mcpConfig) {
    this.name = name;
    this.mcpConfig = mcpConfig;
    this.proc = null;
    this.pending = new Map();
    this.nextId = 1;
    this.tools = [];
    this.ready = false;
  }

  async start() {
    const command = normalizeCommand(this.name, this.mcpConfig.command);
    const args = this.mcpConfig.args || [];
    const env = { ...process.env, ...resolveEnvValues(this.mcpConfig.env) };

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error(`startup timeout (${INIT_TIMEOUT_MS}ms)`));
      }, INIT_TIMEOUT_MS);

      try {
        this.proc = spawn(command, args, { env, stdio: ['pipe', 'pipe', 'pipe'] });
      } catch (err) {
        clearTimeout(timer);
        return reject(err);
      }

      this.proc.on('error', (err) => {
        console.error(`[${this.name}] spawn error: ${err.message}`);
        clearTimeout(timer);
        if (!this.ready) reject(err);
      });

      this.proc.on('exit', (code) => {
        console.log(`[${this.name}] exited (code ${code})`);
        this.ready = false;
        // Reject any pending requests
        for (const [id, { reject: rej }] of this.pending) {
          rej(new Error(`process exited (code ${code})`));
          this.pending.delete(id);
        }
      });

      // Log stderr for diagnostics
      this.proc.stderr?.on('data', (chunk) => {
        const msg = chunk.toString().trim();
        if (msg) console.error(`[${this.name}] ${msg}`);
      });

      // Read newline-delimited JSON from stdout
      const rl = createInterface({ input: this.proc.stdout, crlfDelay: Infinity });
      rl.on('line', (line) => {
        const trimmed = line.trim();
        if (!trimmed) return;
        try {
          const msg = JSON.parse(trimmed);
          if (msg.id != null && this.pending.has(msg.id)) {
            const { resolve: res, timer: t } = this.pending.get(msg.id);
            clearTimeout(t);
            this.pending.delete(msg.id);
            res(msg);
          }
        } catch {
          // Ignore non-JSON lines (debug output from some servers)
        }
      });

      // MCP handshake: initialize → notifications/initialized → tools/list
      this._send('initialize', {
        protocolVersion: '2024-11-05',
        capabilities: {},
        clientInfo: { name: 'toolshed', version: '1.0.0' },
      })
        .then(async () => {
          this._notify('notifications/initialized');
          const resp = await this._send('tools/list', {});
          const raw = resp.result?.tools || [];
          this.tools = raw.map((t) => ({
            name: `${this.name}__${t.name}`,
            description: t.description || '',
            inputSchema: t.inputSchema || { type: 'object', properties: {} },
            _original: t.name,
            _server: this.name,
          }));
          this.ready = true;
          clearTimeout(timer);
          console.log(`[${this.name}] ready — ${this.tools.length} tools`);
          resolve();
        })
        .catch((err) => {
          clearTimeout(timer);
          reject(err);
        });
    });
  }

  _send(method, params) {
    return new Promise((resolve, reject) => {
      if (!this.proc || this.proc.exitCode !== null) {
        return reject(new Error('process not running'));
      }
      const id = this.nextId++;
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`timeout: ${method}`));
      }, CALL_TIMEOUT_MS);
      this.pending.set(id, { resolve, reject, timer });
      this.proc.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
    });
  }

  _notify(method, params) {
    if (!this.proc || this.proc.exitCode !== null) return;
    const msg = { jsonrpc: '2.0', method };
    if (params) msg.params = params;
    this.proc.stdin.write(JSON.stringify(msg) + '\n');
  }

  async callTool(originalName, args) {
    if (!this.ready) throw Object.assign(new Error('server not ready'), { code: -32001 });
    const resp = await this._send('tools/call', { name: originalName, arguments: args });
    if (resp.error) {
      throw Object.assign(new Error(resp.error.message), { code: resp.error.code });
    }
    return resp.result;
  }

  stop() {
    if (this.proc && this.proc.exitCode === null) {
      this.proc.kill('SIGTERM');
    }
  }
}

// ---------------------------------------------------------------------------
// Native tool support — wraps bash scripts as MCP tools
// ---------------------------------------------------------------------------

function buildNativeToolDefs(name, config) {
  const defs = [];
  const runScript = path.join(TOOLS_DIR, name, 'run');

  for (const [cmdName, cmdDef] of Object.entries(config.commands || {})) {
    const properties = {};
    const required = [];
    const positionalOrder = [];

    for (const [argName, argDef] of Object.entries(cmdDef.args || {})) {
      properties[argName] = {
        type: argDef.type || 'string',
        description: argDef.description || '',
      };
      if (argDef.required) required.push(argName);
      if (argDef.positional) positionalOrder.push(argName);
    }

    defs.push({
      name: `${name}__${cmdName}`,
      description: cmdDef.description || '',
      inputSchema: {
        type: 'object',
        properties,
        ...(required.length > 0 ? { required } : {}),
      },
      _native: true,
      _runScript: runScript,
      _command: cmdName,
      _positionalOrder: positionalOrder,
    });
  }

  return defs;
}

function callNativeTool(toolDef, args) {
  return new Promise((resolve) => {
    const cmdArgs = [toolDef._command];

    // Positional args first (in definition order)
    for (const name of toolDef._positionalOrder) {
      if (args[name] != null) cmdArgs.push(String(args[name]));
    }

    // Named (non-positional) args
    for (const [k, v] of Object.entries(args || {})) {
      if (toolDef._positionalOrder.includes(k)) continue;
      cmdArgs.push(`--${k}`, String(v));
    }

    execFile(toolDef._runScript, cmdArgs, {
      timeout: CALL_TIMEOUT_MS,
      maxBuffer: 2 * 1024 * 1024,
      env: process.env,
    }, (err, stdout, stderr) => {
      if (err) {
        resolve({
          content: [{ type: 'text', text: stderr || err.message }],
          isError: true,
        });
      } else {
        resolve({ content: [{ type: 'text', text: stdout }] });
      }
    });
  });
}

// ---------------------------------------------------------------------------
// JSON-RPC helpers
// ---------------------------------------------------------------------------

function jsonRpcOk(id, result) {
  return JSON.stringify({ jsonrpc: '2.0', id, result });
}

function jsonRpcError(id, code, message) {
  return JSON.stringify({ jsonrpc: '2.0', id: id ?? null, error: { code, message } });
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  const mcpServers = new Map();
  const toolIndex = new Map(); // name → tool entry (unified lookup)

  // Discover tools from TOOLS_DIR
  let entries;
  try {
    entries = fs.readdirSync(TOOLS_DIR, { withFileTypes: true }).filter((e) => e.isDirectory());
  } catch (err) {
    console.error(`cannot read ${TOOLS_DIR}: ${err.message}`);
    entries = [];
  }

  // Separate MCP and native tool configs
  const mcpConfigs = [];
  for (const entry of entries) {
    const configPath = path.join(TOOLS_DIR, entry.name, 'tool.json');
    let config;
    try {
      config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
    } catch {
      continue;
    }

    const name = config.name || entry.name;

    if (config.type === 'mcp' && config.mcp) {
      mcpConfigs.push({ name, mcp: config.mcp });
    } else if (config.type === 'native') {
      const runScript = path.join(TOOLS_DIR, entry.name, 'run');
      if (!fs.existsSync(runScript)) {
        console.error(`[${name}] skipped — missing run script`);
        continue;
      }
      const defs = buildNativeToolDefs(name, config);
      for (const d of defs) toolIndex.set(d.name, d);
      console.log(`[${name}] ready — ${defs.length} native commands`);
    }
  }

  // Start MCP servers in parallel
  const results = await Promise.allSettled(
    mcpConfigs.map(async ({ name, mcp }) => {
      const server = new StdioMcpServer(name, mcp);
      await server.start();
      return { name, server };
    }),
  );
  for (const r of results) {
    if (r.status === 'fulfilled') {
      const { name, server } = r.value;
      mcpServers.set(name, server);
      for (const t of server.tools) toolIndex.set(t.name, t);
    } else {
      console.error(`skipped — ${r.reason?.message}`);
    }
  }

  // Cache the tools/list response (immutable after startup)
  const toolsListResponse = {
    tools: [...toolIndex.values()].map((t) => ({
      name: t.name,
      description: t.description,
      inputSchema: t.inputSchema,
    })),
  };

  console.log(`toolshed: ${toolIndex.size} tools registered`);

  // ── HTTP JSON-RPC server ────────────────────────────────────────────────

  const server = http.createServer(async (req, res) => {
    // GET → minimal SSE endpoint (protocol requirement)
    if (req.method === 'GET') {
      res.writeHead(200, { 'Content-Type': 'text/event-stream' });
      res.end();
      return;
    }

    if (req.method !== 'POST') {
      res.writeHead(405);
      res.end();
      return;
    }

    // Read body with size limit
    let body = '';
    let overflow = false;
    for await (const chunk of req) {
      body += chunk;
      if (body.length > MAX_BODY_BYTES) { overflow = true; break; }
    }
    if (overflow) {
      res.writeHead(413, { 'Content-Type': 'application/json' });
      res.end(jsonRpcError(null, -32600, 'Request too large'));
      return;
    }

    let rpc;
    try {
      rpc = JSON.parse(body);
    } catch {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(jsonRpcError(null, -32700, 'Parse error'));
      return;
    }

    const { id, method, params } = rpc;

    try {
      let result;

      switch (method) {
        case 'initialize':
          result = {
            protocolVersion: '2024-11-05',
            capabilities: { tools: { listChanged: false } },
            serverInfo: { name: 'toolshed', version: '1.0.0' },
          };
          break;

        case 'notifications/initialized':
          res.writeHead(204);
          res.end();
          return;

        case 'tools/list':
          result = toolsListResponse;
          break;

        case 'tools/call': {
          const toolName = params?.name;
          if (!toolName) {
            throw Object.assign(new Error('missing tool name'), { code: -32602 });
          }

          const tool = toolIndex.get(toolName);
          if (!tool) {
            throw Object.assign(new Error(`unknown tool: ${toolName}`), { code: -32001 });
          }

          const callArgs = params?.arguments || {};

          if (tool._native) {
            result = await callNativeTool(tool, callArgs);
          } else {
            const mcpServer = mcpServers.get(tool._server);
            if (!mcpServer?.ready) {
              throw Object.assign(new Error(`server not ready: ${tool._server}`), { code: -32001 });
            }
            result = await mcpServer.callTool(tool._original, callArgs);
          }
          break;
        }

        default:
          throw Object.assign(new Error(`method not found: ${method}`), { code: -32601 });
      }

      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(jsonRpcOk(id, result));
    } catch (err) {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(jsonRpcError(id, err.code || -32603, err.message || String(err)));
    }
  });

  server.listen(PORT, '0.0.0.0', () => {
    console.log(`toolshed listening on :${PORT}`);
  });

  // Graceful shutdown
  const shutdown = (sig) => {
    console.log(`shutdown (${sig})`);
    for (const s of mcpServers.values()) s.stop();
    server.close(() => process.exit(0));
  };
  process.on('SIGTERM', () => shutdown('SIGTERM'));
  process.on('SIGINT', () => shutdown('SIGINT'));
}

main().catch((err) => {
  console.error('fatal:', err);
  process.exit(1);
});

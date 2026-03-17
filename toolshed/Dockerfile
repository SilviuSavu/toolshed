FROM node:22-bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl jq git ca-certificates sqlite3 python3 \
    && rm -rf /var/lib/apt/lists/*

# Pre-install known MCP server packages so stdio spawns don't need npx downloads
RUN npm install -g \
    @modelcontextprotocol/server-filesystem@latest \
    @modelcontextprotocol/server-git@latest \
    @fredko/vault-mcp-server@latest \
    @ylin6/unleash-ff-mcp-server@latest

WORKDIR /app
COPY package.json server.js vault-env.sh ./
RUN chmod +x vault-env.sh

EXPOSE 8081

HEALTHCHECK --interval=30s --timeout=10s --retries=3 \
    CMD curl -sf -X POST http://localhost:8081 \
        -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","id":0,"method":"tools/list"}' || exit 1

ENTRYPOINT ["/app/vault-env.sh"]
CMD ["node", "server.js"]

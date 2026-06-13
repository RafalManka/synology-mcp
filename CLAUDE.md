# synology-mcp

A Rust MCP server for Synology NAS (DSM 7). Single static binary. No Python, no Node,
no runtime dependencies. Runs natively on the Synology as a Container Manager project.

The first Rust implementation in this space. Existing alternatives (atom2ueki/mcp-server-synology,
cmeans/mcp-synology, Do-Boo/MCP-SynoLink) are all Python or Node and focus on File Station
and Download Station. This covers the gaps: Container Manager, Storage Manager, disk health,
and system monitoring — the operations home lab users actually need for NAS management.


## What this covers that nothing else does

- Container Manager — list containers, fetch logs, stats, start/stop/restart
- Storage Manager — volume usage, disk health, SMART status
- System — CPU, memory, uptime, DSM version, running packages
- File Station — list directory (read-only, v1 scope)

Intentionally excluded from v1: Download Station (already well covered elsewhere),
Surveillance Station, Photos, any write operations on files.


## Architecture

Two backends, one server:

1. Docker socket (/var/run/docker.sock) — Container Manager tools
   Uses bollard crate. No auth, no HTTP. Direct socket access.
   Only available when running on the Synology itself.

2. DSM HTTP API (SYNO.* API) — everything else
   Session-token auth. HTTPS to localhost (127.0.0.1:5001) when on-NAS,
   or to NAS IP when running remotely.
   Endpoints: /webapi/entry.cgi

Transport: stdio only. Claude Desktop launches the server as a subprocess.
No port exposed. No HTTP server in this binary.


## Crate versions (pinned — 2026-06)

rmcp = "1.7.0"             features = ["server", "macros", "transport-io", "schemars"]
bollard = "0.21.0"         Docker Engine API — Container Manager tools
reqwest = "0.12"           features = ["json", "rustls-tls"]  — DSM API calls
tokio = "1.52.3"           features = ["macros", "rt-multi-thread", "io-std"]
serde = "1.0.228"          features = ["derive"]
serde_json = "1.0.150"
anyhow = "1.0.102"
tracing = "0.1.44"
tracing-subscriber = "0.3.23"   features = ["env-filter", "fmt"]
schemars = "1.0"
tokio-util = "0.7"         features = ["codec"]  — needed by rmcp transport-io


## Project structure

synology-mcp/
Cargo.toml
CLAUDE.md
Dockerfile
docker-compose.yml
README.md
src/
main.rs               entry point — builds DsmClient + Docker, serves stdio
server.rs             SynologyMcp struct, all #[tool_router] impls, ServerHandler
dsm/
mod.rs
client.rs           DsmClient — session auth, raw API call, logout on drop
auth.rs             login/logout — SYNO.API.Auth
storage.rs          volume list, disk list, SMART — SYNO.Storage.CGI.Storage
system.rs           CPU, memory, uptime, packages — SYNO.Core.System
docker/
mod.rs
containers.rs       list, logs, stats, start/stop/restart — bollard
images.rs           list images


## DSM API auth pattern

DSM 7 uses session tokens via SYNO.API.Auth.

Login request (POST /webapi/entry.cgi):
api=SYNO.API.Auth&version=7&method=login
&account=<user>&passwd=<pass>&session=SynologyMCP&format=sid

Response: { "data": { "sid": "<session_id>" }, "success": true }

All subsequent calls include: &_sid=<session_id>

DsmClient holds the sid in an Arc<RwLock<String>>.
On Drop, DsmClient calls logout (best-effort, ignore errors).

Credentials come from environment variables only — never config files, never CLI args:
SYNOLOGY_HOST     e.g. 127.0.0.1 or 192.168.1.100
SYNOLOGY_PORT     default 5001 (HTTPS) or 5000 (HTTP)
SYNOLOGY_USER
SYNOLOGY_PASSWORD
SYNOLOGY_HTTPS    true/false, default true

When running on-NAS: SYNOLOGY_HOST=127.0.0.1, SYNOLOGY_PORT=5001


## MCP tools — complete v1 list

Container tools (via Docker socket):

list_containers
params:  { all: Option<bool> }   all=false returns running only
returns: JSON array of { id, name, image, status, state, created_at, uptime_secs }

get_container_logs
params:  { name: String, tail: Option<u32>, since_seconds: Option<i64> }
tail default 100, max 2000
returns: plain text, stdout+stderr interleaved

get_container_stats
params:  { name: String }
returns: { name, cpu_percent, mem_used_mb, mem_limit_mb, mem_percent,
net_rx_mb, net_tx_mb, blk_read_mb, blk_write_mb }
single snapshot (bollard one_shot=true)

start_container
params:  { name: String }
returns: "Started <name>"

stop_container
params:  { name: String, timeout_secs: Option<u32> }   default 10s
returns: "Stopped <name>"

restart_container
params:  { name: String }
returns: "Restarted <name>"

list_images
params:  none
returns: JSON array of { id, tags, size_mb, created_at }

Storage tools (via DSM API):

get_volumes
DSM: SYNO.Storage.CGI.Storage, method=load_info
returns: JSON array of { name, path, total_gb, used_gb, free_gb, percent_used,
status, fs_type }

get_disks
DSM: SYNO.Storage.CGI.Storage, method=load_info
returns: JSON array of { id, name, model, serial, temp_c, status, smart_status,
size_gb, location }

get_smart_info
params:  { disk_id: String }   e.g. "sda"
DSM: SYNO.Storage.CGI.Smart, method=start + get_info
returns: { disk_id, overall_status, attributes: [{ name, value, worst, threshold, status }] }

System tools (via DSM API):

get_system_info
DSM: SYNO.Core.System, method=info
returns: { model, serial, dsm_version, uptime_secs, hostname }

get_system_utilisation
DSM: SYNO.Core.System.Utilisation, method=get
returns: { cpu_percent, memory_total_mb, memory_used_mb, memory_percent,
network_rx_kbps, network_tx_kbps }

list_packages
DSM: SYNO.Core.Package, method=list
returns: JSON array of { name, version, status, description }

File Station (read-only):

list_directory
params:  { path: String }   e.g. "/volume1/docker"
DSM: SYNO.FileStation.List, method=list
returns: JSON array of { name, path, is_dir, size_bytes, modified_at }


## Key implementation notes

SynologyMcp struct:

    #[derive(Clone)]
    pub struct SynologyMcp {
        docker: Arc<Docker>,          // None if socket unavailable — container tools degrade gracefully
        dsm: Arc<DsmClient>,
        tool_router: ToolRouter<SynologyMcp>,
    }

Docker socket optional:
Try Docker::connect_with_socket_defaults() at startup.
If it fails (e.g. running off-NAS without socket mount), store None.
Container tools return a clear error: "Docker socket not available —
is the server running on the Synology with the socket mounted?"

Error handling:
anyhow::Error inside dsm/ and docker/ modules.
Convert at tool boundary:
.map_err(|e| McpError::internal_error(e.to_string(), None))
Never panic. All errors surface as MCP tool errors.

Logging:
tracing to stderr only. stdout is reserved for MCP protocol.
RUST_LOG=debug shows all DSM API requests (passwords masked in DsmClient).

SMART note:
SYNO.Storage.CGI.Smart requires two calls — start a test then poll get_info.
For v1, call start with type="quick" then immediately get_info.
Response time is ~2-3 seconds. Acceptable for MCP.


## Build and deployment

Cross-compilation target: x86_64-unknown-linux-musl (static binary, no glibc)
Build: cargo build --release --target x86_64-unknown-linux-musl

Dockerfile:
FROM scratch
COPY target/x86_64-unknown-linux-musl/release/synology-mcp /synology-mcp
ENTRYPOINT ["/synology-mcp"]

docker-compose.yml (deploy to Synology Container Manager):
services:
synology-mcp:
image: synology-mcp:latest
environment:
- SYNOLOGY_HOST=127.0.0.1
- SYNOLOGY_PORT=5001
- SYNOLOGY_USER=mcp_user
- SYNOLOGY_PASSWORD=${SYNOLOGY_PASSWORD}
- SYNOLOGY_HTTPS=true
- RUST_LOG=info
volumes:
- /var/run/docker.sock:/var/run/docker.sock
stdin_open: true
restart: unless-stopped
network_mode: host    # needed for 127.0.0.1 to reach DSM API

Claude Desktop config:
{
"mcpServers": {
"synology": {
"command": "docker",
"args": ["exec", "-i", "synology-mcp-synology-mcp-1", "/synology-mcp"]
}
}
}

Security note for README:
Create a dedicated DSM user for MCP with minimal permissions.
Do NOT use admin account. Required permissions: File Station (read),
Storage Manager (read), Docker (only if container tools needed).
Do NOT enable 2FA on the MCP user — DSM API auth does not support it.


## Build order

Session 1 — scaffold + DSM auth + one tool:
Cargo.toml with all dependencies
dsm/client.rs — DsmClient with login/logout, raw API call method
dsm/system.rs — get_system_info only
server.rs — SynologyMcp with one tool (get_system_info), ServerHandler impl
main.rs — read env vars, build DsmClient, serve stdio
Smoke test: SYNOLOGY_HOST=... SYNOLOGY_USER=... SYNOLOGY_PASSWORD=... cargo run

Session 2 — remaining DSM tools:
dsm/storage.rs — get_volumes, get_disks, get_smart_info
dsm/system.rs — get_system_utilisation, list_packages
dsm/auth.rs — extract login/logout from client.rs if it grows large
File Station — list_directory

Session 3 — Docker socket tools:
docker/containers.rs — all six container tools
docker/images.rs — list_images
Optional Docker in SynologyMcp with graceful degradation

Session 4 — deployment + polish:
Dockerfile (FROM scratch)
docker-compose.yml with network_mode: host
Test end-to-end on Synology
Claude Desktop config verified

Session 5 — GitHub:
README.md (see structure below)
Demo gif/screenshot — Claude asking "show me disk health" and getting SMART data
Pin repo, update LinkedIn

## README structure

1. What it does — one paragraph. Rust, Synology DSM 7, no runtime, runs on-NAS.
   Mention the gap it fills vs existing Python/Node alternatives.
2. Demo — gif or screenshot. "Is my NAS healthy?" getting volume + disk + SMART response.
   This is the most important part. Do not ship without it.
3. Supported DSM version — tested on DSM 7.2+
4. Prerequisites — Synology NAS, Container Manager enabled, dedicated DSM user
5. Installation — docker-compose.yml snippet, env vars, Claude Desktop config
6. Tools reference — table: tool | description | key params
7. Building from source — cargo build command, cross-compilation note
8. Why Rust — single binary, no runtime, runs from scratch container, ~5MB image
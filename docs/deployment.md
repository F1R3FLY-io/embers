# Embers Deployment Guide

This guide covers running embers and the frontend using pre-built Docker images. For local development from source, see the [README](../README.md).

All images are available from the [f1r3flyio organization on Docker Hub](https://hub.docker.com/u/f1r3flyio).

## Prerequisites

A running f1r3fly shard with at least one validator and one observer node. Embers needs to reach these nodes by hostname over a shared Docker network.

## 1. Configure environment

Create an `embers.env` file with your cluster connection details. See [embers.env.example](../embers.env.example) for all required variables with descriptions.

Minimum required variables:

| Variable | Description |
|---|---|
| `EMBERS__PORT` | Listening port (default: `3000`) |
| `EMBERS__ADDRESS` | Bind address (default: `::`) |
| `EMBERS__LOG_LEVEL` | Log level (`info`, `debug`, etc.) |
| `EMBERS__AES_ENCRYPTION_KEY` | AES-256 key, 64 hex chars |
| `EMBERS__MAINNET__DEPLOY_SERVICE_URL` | Validator gRPC DeployService (`http://<host>:40401`) |
| `EMBERS__MAINNET__PROPOSE_SERVICE_URL` | Validator gRPC ProposeService (`http://<host>:40402`) |
| `EMBERS__MAINNET__VALIDATOR_WS_API_URL` | Validator WebSocket events (`ws://<host>:40405`) |
| `EMBERS__MAINNET__OBSERVER_URL` | Observer HTTP REST API (`http://<host>:40403`) |
| `EMBERS__MAINNET__OBSERVER_WS_API_URL` | Observer WebSocket events (`ws://<host>:40405`) |
| `EMBERS__MAINNET__SERVICE_KEY` | Funded wallet private key (secp256k1, 64 hex chars) |
| `EMBERS__MAINNET__WALLETS_ENV_KEY` | Wallets encryption key (64 hex chars) |
| `EMBERS__MAINNET__AGENTS_ENV_KEY` | Agents encryption key (64 hex chars) |
| `EMBERS__MAINNET__AGENTS_TEAMS_ENV_KEY` | Agent teams encryption key (64 hex chars) |
| `EMBERS__MAINNET__OSLFS_ENV_KEY` | OSLFS encryption key (64 hex chars) |
| `EMBERS__TESTNET__DEPLOY_SERVICE_URL` | Testnet validator gRPC DeployService |
| `EMBERS__TESTNET__PROPOSE_SERVICE_URL` | Testnet validator gRPC ProposeService |
| `EMBERS__TESTNET__VALIDATOR_WS_API_URL` | Testnet validator WebSocket events |
| `EMBERS__TESTNET__OBSERVER_URL` | Testnet observer HTTP REST API |
| `EMBERS__TESTNET__OBSERVER_WS_API_URL` | Testnet observer WebSocket events |
| `EMBERS__TESTNET__SERVICE_KEY` | Testnet funded wallet private key |
| `EMBERS__TESTNET__ENV_KEY` | Testnet encryption key (64 hex chars) |

For single-cluster setups, mainnet and testnet can point at the same nodes.

## 2. Run the backend (embers)

```bash
docker run -d \
  --env-file ./embers.env \
  --network <shard-network> \
  -p 3000:3000 \
  --name embers \
  f1r3flyio/embers:latest
```

Replace `<shard-network>` with the Docker network your shard is running on (e.g. `f1r3fly`). This allows embers to reach nodes by container hostname.

Verify:

```bash
docker logs embers 2>&1 | grep -E '(ERROR|INFO)'
```

You should see:
```
INFO poem::server: listening addr=socket://[::]:3000
INFO poem::server: server started
```

## 3. Run the frontend (embers-frontend)

```bash
docker run -d \
  -p 8080:80 \
  -e API_URL="http://<embers-host>:3000" \
  --name embers-frontend \
  f1r3flyio/embers-frontend:latest
```

Replace `<embers-host>` with the address where embers is reachable from the browser (e.g. `localhost` for local dev, or a public IP for remote deployments).

Access the frontend at `http://localhost:8080`.

## API endpoints

- **Swagger UI**: `http://<embers-host>:3000/swagger-ui/index.html`
- **OpenAPI spec (JSON)**: `http://<embers-host>:3000/swagger-ui/openapi.json`
- **OpenAPI spec (YAML)**: `http://<embers-host>:3000/swagger-ui/openapi.yaml`

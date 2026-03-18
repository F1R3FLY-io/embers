# embers

F1R3Sky wallets and agents API — a Rust backend (Poem framework) that bridges web clients to the f1r3fly blockchain via gRPC and REST.

## Local Development

### Prerequisites

- Docker Desktop running
- A f1r3fly shard running (bootstrap + validators + observer on the `f1r3fly` Docker network)

### 1. Create environment file

Copy the example and fill in your values:

```bash
cp embers.env.example embers.env
```

Key settings:

- **Deploy/propose** go to a validator: `http://rnode.validator1:40401` / `:40402`
- **Reads** go to the observer: `http://rnode.readonly:40403`
- **WebSocket** endpoints: `ws://rnode.validator1:40405` and `ws://rnode.readonly:40405`
- **SERVICE_KEY**: Use the bootstrap wallet private key from the shard's genesis
- **Mainnet and testnet** can point at the same cluster for local dev

See [embers.env.example](embers.env.example) for all required variables with descriptions.

### 2. Build the Docker image

Build from local source (includes any local code changes):

```bash
docker build -f docker/embers.dockerfile -t f1r3flyio/embers:local .
```

Or use the pre-built image from Docker Hub (may not include recent fixes):

```bash
docker pull f1r3flyio/embers:latest
```

### 3. Run embers

Start detached on the same Docker network as the shard:

```bash
docker run -d \
  --env-file ./embers.env \
  --network f1r3fly \
  -p 8080:3000 \
  --name embers \
  f1r3flyio/embers:local
```

- `--network f1r3fly` — joins the shard's Docker network so embers can reach nodes by container hostname (e.g. `rnode.validator1`)
- `-p 8080:3000` — maps host port 8080 to embers' internal port 3000 (avoids conflict with Grafana on host port 3000)

### 4. Verify

Check container is running:

```bash
docker ps --filter name=embers
```

Check logs for successful bootstrap:

```bash
docker logs embers 2>&1 | grep -E '(ERROR|INFO)'
```

You should see:
```
INFO poem::server: listening addr=socket://[::]:3000
INFO poem::server: server started
```

Test the API:

```bash
curl http://localhost:8080/swagger-ui/openapi.json | head -5
```

### 5. Stop / restart

```bash
docker stop embers
docker rm embers
# Then re-run step 3
```

## API

- **Swagger UI**: http://localhost:8080/swagger-ui/index.html
- **OpenAPI spec (JSON)**: http://localhost:8080/swagger-ui/openapi.json
- **OpenAPI spec (YAML)**: http://localhost:8080/swagger-ui/openapi.yaml

## Additional docs

- [Deployment guide](./docs/deployment.md) — running with pre-built Docker images
- [Node compatibility](./docs/node-compatibility.md) — known incompatibilities with current f1r3fly node
- [Node update log](./docs/embers-rust-node-updates.md) — changes made to get embers working against the current node

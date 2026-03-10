---
description: Manage the e2e test infrastructure using Tilt and run end-to-end tests. Use when the user says "start e2e", "tilt up", "run e2e tests", "e2e status", "tear down tilt", "stop tilt", "tilt logs", "start the infrastructure", "bring up the stack", "is tilt running", or any variation of starting, stopping, checking, or running the end-to-end test environment. Also trigger when the user mentions running the full test suite against the live service, or wants to see if their changes work end-to-end.
user-invocable: true
---

# E2E Test Infrastructure

Manage the Tilt-based development environment and run end-to-end tests against the live service. The environment includes PostgreSQL, ClickHouse, NATS, EMQX, OpenTelemetry, and the ponix-all-in-one service.

## Steps

### 1. Determine the Action

Map the user's request to one of these actions. If ambiguous, ask.

| Action | When to use |
|--------|-------------|
| **start** | User wants to bring up the Tilt environment |
| **test** | User wants to run e2e tests (starts Tilt first if needed) |
| **status** | User wants to check if Tilt / services are healthy |
| **logs** | User wants to see logs from a specific service |
| **stop** | User wants to tear down the Tilt environment |

**Argument mapping examples:**
- `/e2e` → ask user or default to test
- `/e2e start` → start the environment
- `/e2e test` → run e2e tests
- `/e2e status` → check status
- `/e2e logs` → show logs (ask which service)
- `/e2e stop` → tear down

### 2. Check Current Tilt Status

Before starting or running tests, check if Tilt is already running:

```bash
tilt get session 2>/dev/null
```

- If Tilt is already running and healthy, skip starting it.
- If Tilt is not running and the action requires it, start it.

### 3. Execute the Action

#### Start

```bash
# Start Tilt in CI mode (non-interactive, exits when ready)
tilt ci &
TILT_PID=$!
```

Wait for all resources to become ready by polling:

```bash
tilt wait --for=condition=Ready --timeout=120s uiresource/ponix-all-in-one
```

If startup fails, check logs for the failing resource:

```bash
tilt logs --resource=ponix-all-in-one
```

Report which resources are up and their status.

#### Test

1. Ensure Tilt is running (start if not, reuse if already up)
2. Verify the gRPC endpoint is reachable:
   ```bash
   grpcurl -plaintext localhost:50051 list
   ```
3. Run the e2e test suite:
   ```bash
   mise run test:e2e
   ```
4. Report results — number of tests passed/failed/skipped

#### Status

```bash
tilt get uiresources
```

Show a summary of each resource's status (ready/pending/error).

#### Logs

Ask which service if not specified. Valid services: `ponix-all-in-one`, `nats`, `clickhouse`, `postgres`, `otel-lgtm`, `emqx`.

```bash
tilt logs --resource=<service-name>
```

#### Stop

```bash
tilt down
```

Confirm with the user before tearing down, since this destroys all containers and data.

### 4. Report Results

- **Start**: list all resources and their status
- **Test**: show pass/fail counts and any failing test details
- **Status**: show resource health table
- **Logs**: show the relevant log output
- **Stop**: confirm environment was torn down

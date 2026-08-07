# Delete an agent

Deletes an agent from the caller's collection.

| | |
| --- | --- |
| Operation | `POST /api/ai-agents/{id}/delete/prepare` + `POST /api/ai-agents/{id}/delete/send` (tag: AIAgents) |
| Generated client | `AIAgentsApi.agentsPrepareDelete`, `AIAgentsApi.agentsSendDelete` |
| SDK wrapper | `AgentsApiSdk.delete` (`sdk.agents.delete(id)`) |
| Backend | `packages/embers/src/api/agents/endpoints.rs` → `domain/agents/delete.rs` |
| Schema pin | `packages/client/schema.json` @ `69012a4` (Embers API 0.1.0) |

## Semantics

The **tokenless delete flow**, identical in shape to
[OSLF delete](../oslfs/delete.md#semantics): a bodyless prepare returning the bare
`DeleteAgentResp { contract }` (no wrapper, no token), and a send whose body is the bare
`SignedContract`. The asymmetry's scope and its on-chain signature protection are analyzed
there and in [the protocol specification](../../http-api.md#the-two-phase-prepare-sign-send-protocol).

What is specific to agents is the **blast radius**: teams reference agents by pinned
`(id, version)`. Before deleting, call
[`GET /api/ai-agents-teams/{address}/referencing/{agent_id}`](../ai-agents-teams/referencing.md) —
its documented purpose is exactly "so deleting that agent can name what it would break
before doing it".

## Request

### `POST /api/ai-agents/{id}/delete/prepare`

| Parameter | In | Type | Required | Meaning |
| --- | --- | --- | --- | --- |
| `id` | path | `string` (UUIDv4) | yes | The agent to delete. |

No body.

### `POST /api/ai-agents/{id}/delete/send`

Path `id` as above. Body: `SignedContract` only.

### Request types (TypeScript)

Generated declarations from `packages/client/src/api-client/models/`, doc comments included — the comments are generated from the backend models' Rust `///` doc comments (property names are the SDK's camelCase; the wire JSON uses snake_case where they differ):

```typescript
/**
 * A client-signed contract, ready to deploy: the send phase of the two-phase
 * protocol carries it, and the chain is the verifier of record.
 */
export interface SignedContract {
  /** The exact prepared contract bytes that were signed. */
  contract: Base64;
  /** The signing wallet's public key. */
  deployer: Base64;
  /** Signature over the contract's blake2b digest, per `sig_algorithm`. */
  sig: Base64;
  /** The signature algorithm; the platform signs with `secp256k1`. */
  sigAlgorithm: string;
}
```

## Responses

| Phase | Status | Body | Meaning |
| --- | --- | --- | --- |
| prepare | 200 | `DeleteAgentResp` | `{ contract }` — bare unsigned contract bytes. |
| send | 200 | `SendResp` | `{ deploy_id }` — accepted; finalization pending. |
| send | 500 | `InternalError` | Deploy submission failure. |

### Response types (TypeScript)

Generated declarations from `packages/client/src/api-client/models/`, doc comments included — the comments are generated from the backend models' Rust `///` doc comments (property names are the SDK's camelCase; the wire JSON uses snake_case where they differ):

```typescript
export interface DeleteAgentResp {
  /** The unsigned deletion contract bytes to sign. */
  contract: Base64;
}

export interface SendResp {
  /**
   * The accepted deploy's identifier (lowercase hex of its signature);
   * finalization is still pending when this returns.
   */
  deployId: string;
}
```

## Signing and binding

| Property | Value |
| --- | --- |
| Token | **Absent** — the delete asymmetry. |
| Signing key | The owner's wallet key. |
| Contracts signed | One: the deletion contract. |
| Deploy identifier | `hex(sig).toLowerCase()` — subscribed before send. |

## Retry semantics

Domain-idempotent: deleting an already-deleted agent is a no-op on the collection. After a
finalization timeout, a [list](list.md) read shows whether the removal landed.

## Sequence

![Delete agent sequence](../../diagrams/ai-agents/delete.svg)

## Worked example

Shapes are identical to the [OSLF delete example](../oslfs/delete.md#worked-example) with
agent paths substituted.

## SDK usage

```typescript
const referencing = /* GET referencing teams first — see Related */
await sdk.agents.delete(agentId);
```

## Related

- [Referencing teams](../ai-agents-teams/referencing.md) — **check before deleting**.
- [OSLF delete](../oslfs/delete.md) — the tokenless flow analyzed in full.
- [List](list.md) — where the removal becomes observable.

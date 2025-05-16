# bsky architecture

Bsky is based on 4 main components:

1. `PDS` - holds all information about users it serves such as posts, likes, replies, etc.
   PSDs are not synchronized in any way, they just proxy requests and maintain user state.
2. `Relay` - subscribes to all available `PSDs` in the network to gather updates (posts, likes, replies, etc.) into a single stream of updates.
3. `AppView` - Consumes all events from `Relay` to build a personal feed for every user and maintain the state of the social network.
4. `ChatAPI` - Stores users' DMs. It's a proprietary extension to `ATProto` created by bsky. The code is not available in open source.

## PDS and AppView interactions

For all writes and updates, `PDS` acts as a primary replica. It always holds the newest information about the users it serves.
All updates are pushed through the `sync` API to `Relay` and later to `AppView`.

"Create post" is an example of such an operation:

```mermaid
sequenceDiagram
  actor User
  participant Frontend
  participant PDS
  participant PDS-Postgres
  participant Relay
  participant AppView
  participant AppView-Postgres

  User ->> Frontend: Create post
  Frontend ->> PDS: Create post
  PDS ->> PDS-Postgres: INSERT post
  PDS -->> Frontend: OK
  Frontend -->> User: OK
  PDS ->> Relay: Post created event
  Relay ->> AppView: Post created event
  AppView ->> AppView-Postgres: INSERT post
```

In case of reads, `PDS` acts as a proxy to `AppView`.

"Fetch post" is an example of such an operation:

```mermaid
sequenceDiagram
  actor User
  participant Frontend
  participant PDS
  participant AppView
  participant AppView-Postgres

  User ->> Frontend: Fetch post
  Frontend ->> PDS: Fetch post
  PDS ->> AppView: Fetch post
  AppView ->> AppView-Postgres: Fetch post
  AppView-Postgres ->> AppView: post
  AppView -->> PDS: post
  PDS -->> Frontend: post
  Frontend -->> User: post
```

## PDS and Chat interactions

`PDS` acts as a proxy to `Chat API`

Send DM example:

```mermaid
sequenceDiagram
  actor User
  participant Frontend
  participant PDS
  participant Chat

  User ->> Frontend: Send dm
  Frontend ->> PDS: Send dm
  PDS ->> Chat: Send dm
  Chat -->> PDS: OK
  PDS -->> Frontend: OK
  Frontend -->> User: OK
```

Read DM example:

```mermaid
sequenceDiagram
  actor User
  participant Frontend
  participant PDS
  participant Chat

  User ->> Frontend: Read dm
  Frontend ->> PDS: Read dm
  PDS ->> Chat: Read dm
  Chat -->> PDS: dm
  PDS -->> Frontend: dm
  Frontend -->> User: dm
```

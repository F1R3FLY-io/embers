# Replace postgres with Firefly

> **Superseded — this option is now implemented.** What this document sketched as a
> future migration is delivered by two standalone tools: **state-sync** (Postgres →
> Firefly checkpoint/restore) and **events-sync** (ATProto firehose ↔ Firefly). See
> [integrations/f1r3sky-and-atproto.md](integrations/f1r3sky-and-atproto.md),
> [integrations/state-sync.md](integrations/state-sync.md), and
> [integrations/events-sync.md](integrations/events-sync.md). The text below is
> retained as historical design context.

In this scenario postgres is replaced with Firefly. All data is stored and accessed through the Firefly API.
SQL queries are replaced with coresponding Firefly API calls.

```mermaid
sequenceDiagram
  participant PDS
  participant Firefly

  PDS ->> Firefly: Save data
  Firefly ->> PDS: OK
```

Required SQL subset:

- SELECT
- SELECT DISTINCT
- INSERT
- DELETE
- WHERE
- LEFT JOIN
- INNER JOIN
- ON CONFLICT DO NOTHING
- ON CONFLICT DO UPDATE
- RETURNING
- COUNT
- LIMIT
- ORDER BY
- SUBQUERY

# Dump postgres to Firefly every X minutes

In this scenario postgres is used as main database and duplicated to Firefly once in X minutes.

```mermaid
sequenceDiagram
  participant PDS
  participant Postgres
  participant Firefly

  PDS ->> Postgres: Save data
  Postgres ->> PDS: OK

  loop sync every X minutes
    Postgres ->> Firefly: Push updates
  end
```

Can be done as a cron job that runs pg_dump and saves everything to Firefly.

# Run postgres on top of FireflyFS

In this scenario postgres is used as main database that runs on FireflyFS.

```mermaid
sequenceDiagram
  participant PDS
  participant Postgres
  participant FireflyFS
  participant Firefly

  PDS ->> Postgres: Save data
  Postgres ->> FireflyFS: Write file
  FireflyFS ->> Firefly: Write
  Postgres ->> PDS: OK
```

Whe might need expand FireflyFS fuse integration to support more methods.

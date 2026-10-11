# Yellowstone paper feed

The network adapter is feature-gated behind `yellowstone`.

It uses the current official Yellowstone reconnecting client instead of a
hand-written retry loop:

```text
GeyserGrpcClient
  -> subscribe_with_reconnect
  -> ReconnectEvent::Update
  -> FeedEvent::Account

reconnect/fork recovery
  -> ReconnectEvent::DiscardBanks
  -> FeedEvent::DiscardBanks
```

## Version inputs

Every account event retains:

- connection generation
- slot
- bank id when present
- write version
- pubkey / owner
- raw account bytes

Those fields are sufficient to construct the engine's `StateVersion` and to
separate fork-local bank state.

## Fork safety

`DiscardBanks` is never ignored. The adapter forwards the exact
`(generation, slot, bank_id)` identities that Yellowstone says must be
discarded before replacement updates are applied.

The current paper engine deliberately does **not** apply these events directly
to `PoolCell` yet. The next layer is a bank-aware account journal/stager that
can roll back discarded banks and publish only the currently selected latest
snapshot.

## Subscription scope

The adapter accepts exact account addresses and/or owner program addresses and
uses `Processed` commitment for low-latency research.

No signer, private key, transaction builder, or transaction sender exists in
this module.

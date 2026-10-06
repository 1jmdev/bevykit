# bevykit_core

Shared foundations used by every bevykit module:

- **Keys**: compact identifiers built from strings or game-defined enums.
- **Scopes**: entities that own other entities, tasks, and nested scopes; closing a scope
  cleans up everything it owns and blocks late asynchronous results.
- **Scoped tasks**: asynchronous work whose completion is applied to the world as a command.
- **Pause**: reference-counted gameplay pause that keeps presentation responsive.
- **Deadlines**: persistent wall-clock deadlines with a configurable clock-change policy.
- **Schedule ordering**: the documented `KitSystems` sets.

This crate is re-exported by `bevykit`; most games depend on that crate instead.

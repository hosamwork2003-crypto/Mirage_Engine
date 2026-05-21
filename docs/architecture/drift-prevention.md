# Architectural Drift Prevention (Canonical)

This document is the canonical governance artifact for preventing architectural drift in the Mirage runtime.

Forbidden patterns:
- ECS/task-graph orchestration
- Async authority transfer
- Runtime-executed topology mutation from morphogenic
- Hidden authority delegation between crates

Crate authority map is documented in crate-ownership-map.md. Any change must update these docs.

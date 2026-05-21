# Subsystem Boundaries (Canonical)

This document declares forbidden cross-subsystem interactions and must be kept in sync with crate-ownership-map.md.

DO NOT allow runtime mutation of topology from morphogenic crates.
DO NOT allow morphogenic to perform execution.
DO NOT allow mkr-core to leak ownership to other crates except via explicit APIs.

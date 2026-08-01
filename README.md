# muskitty-layout

[![crates.io](https://img.shields.io/crates/v/muskitty-layout.svg)](https://crates.io/crates/muskitty-layout)
[![Documentation](https://docs.rs/muskitty-layout/badge.svg)](https://docs.rs/muskitty-layout)
[![License](https://img.shields.io/crates/l/muskitty-layout.svg)](https://github.com/muskitty-dev/muskitty-layout/blob/main/LICENSE)

CSS Layout engine — converts a DOM tree with per-element ComputedStyle into a
layout box tree and computes positions/sizes using
[taffy 0.12](https://crates.io/crates/taffy) (Flexbox / Grid / Block).

Part of the [MusKitty](https://github.com/muskitty-dev) browser engine project.

## Status

| Component | Spec | Tests |
|-----------|------|-------|
| LayoutTree (taffy TaffyTree + NodeId map) | — | — |
| ComputedStyle → taffy Style mapping | CSS Display §2 / Box Model §2-§3 / Flexbox §4-§8 | 35 |
| DOM + ComputedStyle → LayoutTree conversion | CSS Display §2.4 box tree | — |
| Layout computation (compute_layout) | — | 8 |
| End-to-end integration (HTML+CSS → layout) | — | 7 |
| **Total** | | **50** |

- Zero `unsafe` code
- Zero C/C++ dependencies
- Rust stable toolchain only
- MSRV 1.82

## Pipeline

```text
DOM tree + ComputedStyle per element
    │  build_layout_tree
    ▼
LayoutTree (taffy TaffyTree + NodeId mapping)
    │  compute_layout
    ▼
LayoutResult (per-element x/y/width/height)
```

## Spec coverage

- CSS Display Level 3 §2: box tree generation
- CSS Box Model Level 3 §2 / §3: margin/border/padding/content + box-sizing
- CSS Flexbox Level 1 §4-§8: flex container/item + alignment
- CSS Grid Level 2: grid container/item (via taffy)

## Usage

```toml
[dependencies]
muskitty-layout = "0.1"
```

```rust
use muskitty_layout::{build_layout_tree, compute_layout};
```

## License

Apache-2.0, consistent with all MusKitty crates.

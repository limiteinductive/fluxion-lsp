# Fluxion LSP

This is a language server for the (Fluxion)[https://github.com/finegrain-ai/refiners/tree/main/src/refiners/fluxion] deep learning micro-framework. It is written in Rust and uses the (RustPython)[https://github.com/RustPython/RustPython] interpreter to parse and analyze Python code.

This is highly experimental and not ready for testing yet.

## Installation

To install the language server, you need to have Rust installed. You can then run:

```bash
cargo install --path .
```

## Features

- [ ] Parse Python code
- [ ] Detect Fluxion code
- [ ] Infer shapes of tensors when initializing them
- [ ] Represent Fluxion Chains as a graph
- [ ] Support Context API
- [ ] Support custom fl.Module classes
- [ ] VSCode extension

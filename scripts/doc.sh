#!/usr/bin/env bash
# scripts/doc.sh – generate and serve the Rust API docs

set -euo pipefail

# Build docs for the entire workspace, include private items (useful during development)
cargo doc --workspace --document-private-items

# Open the index page (Linux “xdg-open”, macOS “open”, Windows “start”)
if command -v xdg-open > /dev/null; then
    xdg-open target/doc/index.html
elif command -v open > /dev/null; then
    open target/doc/index.html
else
    echo "Docs generated at: $(realpath target/doc/index.html)"
fi

#!/usr/bin/env bash
set -e

PROJECT_NAME="kora-rent-reclaim"

echo "Creating project structure: $PROJECT_NAME"

# Root
mkdir -p "$PROJECT_NAME"
cd "$PROJECT_NAME"

# Root files
touch Cargo.toml Cargo.lock .env.example README.md

# src root
mkdir -p src

touch src/main.rs
touch src/lib.rs
touch src/config.rs
touch src/error.rs
touch src/utils.rs

# src/solana
mkdir -p src/solana
touch src/solana/mod.rs
touch src/solana/client.rs
touch src/solana/rent.rs
touch src/solana/accounts.rs

# src/kora
mkdir -p src/kora
touch src/kora/mod.rs
touch src/kora/monitor.rs
touch src/kora/types.rs

# src/reclaim
mkdir -p src/reclaim
touch src/reclaim/mod.rs
touch src/reclaim/engine.rs
touch src/reclaim/eligibility.rs
touch src/reclaim/batch.rs

# src/storage
mkdir -p src/storage
touch src/storage/mod.rs
touch src/storage/db.rs
touch src/storage/models.rs

# src/tui
mkdir -p src/tui
touch src/tui/mod.rs
touch src/tui/app.rs
touch src/tui/ui.rs
touch src/tui/event.rs
touch src/tui/theme.rs

# src/tui/components
mkdir -p src/tui/components
touch src/tui/components/mod.rs
touch src/tui/components/header.rs
touch src/tui/components/stats.rs
touch src/tui/components/accounts_table.rs
touch src/tui/components/logs.rs
touch src/tui/components/chart.rs
touch src/tui/components/help.rs

# src/tui/screens
mkdir -p src/tui/screens
touch src/tui/screens/mod.rs
touch src/tui/screens/dashboard.rs
touch src/tui/screens/accounts.rs
touch src/tui/screens/operations.rs
touch src/tui/screens/settings.rs

# src/cli
mkdir -p src/cli
touch src/cli/mod.rs
touch src/cli/commands.rs

# tests
mkdir -p tests/common
touch tests/integration_test.rs
touch tests/common/mod.rs

# docs
mkdir -p docs
touch docs/ARCHITECTURE.md
touch docs/KORA_EXPLAINED.md
touch docs/TUI_GUIDE.md

echo "✅ Project structure created successfully."

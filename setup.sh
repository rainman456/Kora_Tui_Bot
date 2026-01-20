#!/usr/bin/env bash
set -euo pipefail

########################################
# Logging Functions
########################################
log_info() {
    printf "[INFO] %s\n" "$1"
}

log_error() {
    printf "[ERROR] %s\n" "$1" >&2
}

########################################
# OS Detection
########################################
detect_os() {
    local os
    os="$(uname)"
    if [[ "$os" == "Linux" ]]; then
        echo "Linux"
    elif [[ "$os" == "Darwin" ]]; then
        echo "Darwin"
    else
        log_error "Unsupported OS: $os"
        exit 1
    fi
}

########################################
# Detect Shell and Profile File
########################################
detect_shell_profile() {
    local shell_profile
    if [[ -n "${ZSH_VERSION:-}" ]]; then
        shell_profile="$HOME/.zshrc"
    elif [[ -n "${BASH_VERSION:-}" ]]; then
        shell_profile="$HOME/.bashrc"
    else
        shell_profile="$HOME/.profile"
    fi
    echo "$shell_profile"
}

########################################
# Install OS-Specific Dependencies
########################################
install_dependencies() {
    local os="$1"
    if [[ "$os" == "Linux" ]]; then
        log_info "Detected Linux OS. Updating package list and installing dependencies..."
        SUDO=""
        if command -v sudo >/dev/null 2>&1; then
            SUDO="sudo"
        fi
        $SUDO apt-get update 
        $SUDO apt-get install -y \
                build-essential \
                pkg-config \
                libudev-dev \
                llvm \
                libclang-dev \
                protobuf-compiler \
                libssl-dev
    elif [[ "$os" == "Darwin" ]]; then
        log_info "Detected macOS. Installing dependencies via Homebrew..."
        if ! command -v brew >/dev/null 2>&1; then
            log_info "Homebrew not found. Installing Homebrew..."
            /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
        fi
        brew install \
            protobuf \
            openssl \
            pkg-config \
            llvm
        if ! xcode-select -p >/dev/null 2>&1; then
            log_info "Xcode Command Line Tools not found. Installing..."
            xcode-select --install
        fi
    fi

    echo ""
}



########################################
# Install Solana CLI
########################################
install_solana_cli() {
    local os="$1"
    local shell_profile="$2"

    if command -v solana >/dev/null 2>&1; then
        log_info "Solana CLI is already installed. Updating..."
        agave-install update
    else
        log_info "Installing Solana CLI..."
        sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
        log_info "Solana CLI installation complete."
    fi

    # Add Solana to PATH if not already there (and export immediately)
    local solana_path="$HOME/.local/share/solana/install/active_release/bin"
    if [[ ":$PATH:" != *":$solana_path:"* ]]; then
        echo "export PATH=\"$solana_path:\$PATH\"" >> "$shell_profile"
        log_info "Added Solana CLI to PATH in $shell_profile."
        export PATH="$solana_path:$PATH"  # Apply immediately in this script
    fi

    # Source the profile to apply changes immediately (temporarily disable -u for Cloud Shell compatibility)
    set +u
    . "$shell_profile"
    set -u

    # Now check if it's available
    if command -v solana >/dev/null 2>&1; then
        solana --version
    else
        log_error "Solana CLI installation failed."
        exit 1
    fi

    # Configure Solana for devnet
    solana config set --url https://api.devnet.solana.com
    log_info "Solana CLI configured for devnet."

    echo ""
}

########################################
# Install Kora CLI
########################################
install_kora_cli() {
    if command -v kora >/dev/null 2>&1; then
        log_info "Kora CLI is already installed. Skipping..."
    else
        log_info "Installing Kora CLI via Cargo..."
        cargo install kora-cli
        log_info "Kora CLI installation complete."
    fi

    if command -v kora >/dev/null 2>&1; then
        kora --version
    else
        log_error "Kora CLI installation failed."
        exit 1
    fi

    echo ""
}

########################################
# Install Just (for building Kora if needed)
########################################
install_just() {
    if command -v just >/dev/null 2>&1; then
        log_info "Just is already installed. Skipping..."
    else
        log_info "Installing Just via Cargo..."
        cargo install just
        log_info "Just installation complete."
    fi

    if command -v just >/dev/null 2>&1; then
        just --version
    else
        log_error "Just installation failed."
        exit 1
    fi

    echo ""
}

########################################
# Clone Kora Repository (Optional for Source Build)
########################################
clone_kora_repo() {
    local repo_dir="$HOME/kora"
    if [[ -d "$repo_dir" ]]; then
        log_info "Kora repository already cloned at $repo_dir. Pulling latest changes..."
        cd "$repo_dir" && git pull
    else
        log_info "Cloning Kora repository..."
        git clone https://github.com/solana-foundation/kora.git "$repo_dir"
        log_info "Kora repository cloned to $repo_dir."
    fi

    echo ""
}

########################################
# Print Installed Versions
########################################
print_versions() {
    echo ""
    echo "Installed Versions:"
    echo "Rust: $(rustc --version 2>/dev/null || echo 'Not installed')"
    echo "Solana CLI: $(solana --version 2>/dev/null || echo 'Not installed')"
    echo "Kora CLI: $(kora --version 2>/dev/null || echo 'Not installed')"
    echo "Just: $(just --version 2>/dev/null || echo 'Not installed')"
    echo ""
}

########################################
# Main Execution Flow
########################################
main() {
    local os
    os=$(detect_os)
    local shell_profile
    shell_profile=$(detect_shell_profile)

    #install_dependencies "$os"
    #install_rust
    install_solana_cli "$os" "$shell_profile"
    install_kora_cli
    install_just
    clone_kora_repo

    print_versions

    echo "Installation complete. Please restart your terminal or run 'source $shell_profile' to apply all changes."
    echo "Next steps: Create kora.toml and signers.toml in your project dir, then run 'kora rpc start' to start the local node."
}

main "$@"
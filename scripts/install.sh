#!/usr/bin/env bash
# ==============================================================================
#  🌸 Kiyo: Universal Document, E-Book & Comic Workstation
#  One-Line Automated Universal Installer & Updater for Linux
#
#  Usage:
#    curl -fsSL https://raw.githubusercontent.com/Tribal-Chief-001/Kiyo/main/scripts/install.sh | bash
#
#  Uninstall:
#    curl -fsSL https://raw.githubusercontent.com/Tribal-Chief-001/Kiyo/main/scripts/install.sh | bash -s -- --uninstall
# ==============================================================================

set -euo pipefail

REPO="Tribal-Chief-001/Kiyo"
RAW_BASE="https://raw.githubusercontent.com/${REPO}/main"
API_BASE="https://api.github.com/repos/${REPO}"

# Terminal Colors & Styling
BOLD="$(tput bold 2>/dev/null || printf '')"
GREEN="$(tput setaf 2 2>/dev/null || printf '')"
CYAN="$(tput setaf 6 2>/dev/null || printf '')"
YELLOW="$(tput setaf 3 2>/dev/null || printf '')"
RED="$(tput setaf 1 2>/dev/null || printf '')"
RESET="$(tput sgr0 2>/dev/null || printf '')"

log_info() {
    printf "${CYAN}ℹ${RESET}  %s\n" "$*"
}

log_success() {
    printf "${GREEN}✔${RESET}  ${BOLD}%s${RESET}\n" "$*"
}

log_warn() {
    printf "${YELLOW}⚠${RESET}  %s\n" "$*"
}

log_error() {
    printf "${RED}✖${RESET}  ${BOLD}%s${RESET}\n" "$*" >&2
}

# --- Uninstall Logic ---
if [[ "${1:-}" == "--uninstall" ]]; then
    printf "\n${BOLD}${RED}Removing Kiyo from your system...${RESET}\n\n"
    
    rm -f "${HOME}/.local/bin/kiyo"
    rm -f "${HOME}/.local/bin/kiyo-remote"
    rm -f "${HOME}/.local/share/applications/org.kiyo.Kiyo.desktop"
    rm -f "${HOME}/.local/share/nemo/actions/kiyo.nemo_action"
    rm -f "${HOME}/.local/share/icons/hicolor/scalable/apps/org.kiyo.Kiyo.svg"
    rm -f "${HOME}/.local/share/icons/hicolor/*/apps/org.kiyo.Kiyo.png"

    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "${HOME}/.local/share/applications" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
    fi

    log_success "Kiyo and all desktop integrations have been cleanly uninstalled."
    exit 0
fi

# Banner
printf "\n"
printf "${BOLD}${CYAN}"
cat << 'EOF'
  _  ___             
 | |/ (_)_   _  ___  
 | ' /| | | | |/ _ \ 
 | . \| | |_| | (_) |
 |_|\_\_|\__, |\___/ 
         |___/       
EOF
printf "${RESET}"
printf " ${BOLD}Universal Document, E-Book & Comic Workstation${RESET}\n"
printf " https://github.com/%s\n\n" "${REPO}"

# --- Environment Detection ---
OS="$(uname -s)"
ARCH="$(uname -m)"

if [[ "${OS}" != "Linux" ]]; then
    log_error "Kiyo automated installer currently supports Linux. Detected OS: ${OS}"
    exit 1
fi

case "${ARCH}" in
    x86_64)
        TARGET_ARCH="x86_64"
        ;;
    aarch64|arm64)
        TARGET_ARCH="aarch64"
        ;;
    *)
        log_error "Unsupported CPU architecture: ${ARCH}. Kiyo supports x86_64 and aarch64."
        exit 1
        ;;
esac

BIN_DIR="${HOME}/.local/bin"
DESKTOP_DIR="${HOME}/.local/share/applications"
ICONS_DIR="${HOME}/.local/share/icons/hicolor"
NEMO_DIR="${HOME}/.local/share/nemo/actions"

mkdir -p "${BIN_DIR}" "${DESKTOP_DIR}" "${NEMO_DIR}" "${ICONS_DIR}/scalable/apps" "${ICONS_DIR}/256x256/apps"

TMP_DIR="$(mktemp -d -t kiyo-install-XXXXXX)"
trap 'rm -rf "${TMP_DIR}"' EXIT

log_info "Detected Platform: ${BOLD}Linux (${TARGET_ARCH})${RESET}"

# --- Binary Retrieval Strategy ---
# 1. Try downloading pre-built release tarball from GitHub
# 2. Fall back to local build if inside repo or cargo available
INSTALLED_VIA_BINARY=false

log_info "Fetching latest release information..."
RELEASE_TAG="v0.1.0"
TARBALL_NAME="kiyo-linux-${TARGET_ARCH}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${RELEASE_TAG}/${TARBALL_NAME}"

if command -v curl >/dev/null 2>&1; then
    FETCH_CMD="curl -fsSL"
elif command -v wget >/dev/null 2>&1; then
    FETCH_CMD="wget -qO-"
else
    log_error "Neither 'curl' nor 'wget' was found on your system. Please install curl."
    exit 1
fi

log_info "Downloading pre-compiled release (${RELEASE_TAG})..."
if ${FETCH_CMD} "${DOWNLOAD_URL}" > "${TMP_DIR}/${TARBALL_NAME}" 2>/dev/null && [ -s "${TMP_DIR}/${TARBALL_NAME}" ]; then
    log_info "Extracting binaries..."
    tar -xzf "${TMP_DIR}/${TARBALL_NAME}" -C "${TMP_DIR}"
    cp "${TMP_DIR}/kiyo" "${BIN_DIR}/kiyo"
    cp "${TMP_DIR}/kiyo-remote" "${BIN_DIR}/kiyo-remote"
    chmod +x "${BIN_DIR}/kiyo" "${BIN_DIR}/kiyo-remote"
    INSTALLED_VIA_BINARY=true
    log_success "Binaries installed to ${BIN_DIR}/"
else
    log_warn "Pre-built binary tarball not directly reachable at release URL."
    # If Cargo is available, build directly
    if command -v cargo >/dev/null 2>&1; then
        log_info "Rust/Cargo detected on system. Building release binaries from source..."
        cargo install --git "https://github.com/${REPO}.git" kiyo-cli --bin kiyo --bin kiyo-remote --root "${HOME}/.local" --force
        INSTALLED_VIA_BINARY=true
        log_success "Built and installed binaries via Cargo."
    else
        log_error "Could not fetch pre-compiled binaries and Cargo is not installed."
        log_error "Please download from https://github.com/${REPO}/releases or install Rust via https://rustup.rs"
        exit 1
    fi
fi

# --- Desktop Integration Assets ---
log_info "Configuring Linux Mint Cinnamon & Desktop integration..."

# 1. Scalable SVG and PNG Icons
${FETCH_CMD} "${RAW_BASE}/packaging/org.kiyo.Kiyo.svg" > "${ICONS_DIR}/scalable/apps/org.kiyo.Kiyo.svg" || true
${FETCH_CMD} "${RAW_BASE}/packaging/org.kiyo.Kiyo.png" > "${ICONS_DIR}/256x256/apps/org.kiyo.Kiyo.png" || true

# 2. FreeDesktop .desktop Launcher Entry
${FETCH_CMD} "${RAW_BASE}/packaging/org.kiyo.Kiyo.desktop" > "${DESKTOP_DIR}/org.kiyo.Kiyo.desktop" || true

# 3. Linux Mint Nemo Context Actions
${FETCH_CMD} "${RAW_BASE}/packaging/nemo/kiyo.nemo_action" > "${NEMO_DIR}/kiyo.nemo_action" || true

# Update desktop and icon databases if tools exist
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${DESKTOP_DIR}" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${ICONS_DIR}" 2>/dev/null || true
fi

log_success "Desktop shortcuts, application icon, and Nemo context actions installed."

# --- PATH Validation & Auto-Setup ---
PATH_READY=false
if [[ ":${PATH}:" == *":${BIN_DIR}:"* ]]; then
    PATH_READY=true
fi

if [ "${PATH_READY}" = false ]; then
    log_warn "${BIN_DIR} is not in your current \$PATH."
    SHELL_NAME="$(basename "${SHELL:-bash}")"
    CONFIG_FILE="${HOME}/.bashrc"
    
    if [[ "${SHELL_NAME}" == "zsh" ]]; then
        CONFIG_FILE="${HOME}/.zshrc"
    elif [[ -f "${HOME}/.bash_profile" ]]; then
        CONFIG_FILE="${HOME}/.bash_profile"
    fi

    echo "export PATH=\"${BIN_DIR}:\$PATH\"" >> "${CONFIG_FILE}"
    log_info "Appended ${BIN_DIR} to ${CONFIG_FILE}"
    log_info "Run: ${BOLD}source ${CONFIG_FILE}${RESET} to reload your current terminal."
fi

# --- Final Verification ---
printf "\n"
if "${BIN_DIR}/kiyo" --version >/dev/null 2>&1; then
    KIYO_VER="$("${BIN_DIR}/kiyo" --version)"
    printf "${BOLD}${GREEN}======================================================${RESET}\n"
    printf "  🎉 ${BOLD}Kiyo is successfully installed and ready!${RESET}\n"
    printf "  Version: ${CYAN}%s${RESET}\n" "${KIYO_VER}"
    printf "  Location: ${CYAN}%s/kiyo${RESET}\n" "${BIN_DIR}"
    printf "${BOLD}${GREEN}======================================================${RESET}\n\n"
    printf "Quick Commands:\n"
    printf "  ${BOLD}kiyo <file.pdf>${RESET}             Open file in 120 FPS GPU reader\n"
    printf "  ${BOLD}kiyo gui doc1.pdf doc2.epub${RESET} Open multi-file tabbed session\n"
    printf "  ${BOLD}kiyo merge a.pdf b.pdf -o out.pdf${RESET} Merge documents instantaneously\n"
    printf "  ${BOLD}kiyo crop-margins paper.pdf${RESET} Auto-crop margins for compact screens\n"
    printf "  ${BOLD}kiyo impose --booklet doc.pdf${RESET} Prepress saddle-stitch imposition\n"
    printf "  ${BOLD}kiyo --help${RESET}                 View all 11 subcommands\n\n"
    printf "Linux Mint Users:\n"
    printf "  Right-click any PDF in Nemo file manager for instant actions!\n\n"
else
    log_warn "Installation finished, but verification failed. Please check permissions in ${BIN_DIR}."
fi

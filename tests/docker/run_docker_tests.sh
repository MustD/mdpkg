#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

echo "==> Building mdpkg binary on host..."
cargo build
MDPKG_BIN="$REPO_ROOT/target/debug/mdpkg"
test -x "$MDPKG_BIN"

echo "==> Building Ubuntu test image..."
docker build -f tests/docker/Dockerfile.ubuntu -t mdpkg-test-ubuntu:latest tests/docker

echo "==> Running Ubuntu (apt) end-to-end integration test..."
docker run --rm \
    -v "$MDPKG_BIN:/usr/local/bin/mdpkg:ro" \
    mdpkg-test-ubuntu:latest \
    bash -ceu '
        export DEBIAN_FRONTEND=noninteractive
        export MDPKG_NON_INTERACTIVE=1

        mkdir -p /tmp/mdpkg-test
        cat > /tmp/mdpkg-test/packages.md <<EOF
# Packages

## MAIN — Core

### cli
EOF

        echo "[Ubuntu] 1. Add tree via d: (apt) and verify postInstall hook"
        mkdir -p /tmp/mdpkg-test/hooks/tree
        cat > /tmp/mdpkg-test/hooks/tree/postInstall.sh <<"EOF"
#!/usr/bin/env bash
set -euo pipefail
echo "$MDPKG_PACKAGE:$MDPKG_SOURCE:$MDPKG_STAGE" > "$MDPKG_DATA_DIR/tree_post_install.txt"
EOF
        chmod +x /tmp/mdpkg-test/hooks/tree/postInstall.sh

        mdpkg --dir /tmp/mdpkg-test --host ubuntu-test add tree --source d --desc "directory tree"
        command -v tree >/dev/null
        test -x /usr/bin/tree
        grep -q "d:\`tree\`" /tmp/mdpkg-test/packages.md
        grep -q -- "- \[x\] d:\`tree\`" /tmp/mdpkg-test/ubuntu-test.md
        grep -q "tree:apt:postInstall" /tmp/mdpkg-test/tree_post_install.txt

        echo "[Ubuntu] 2. Add custom package my-curl-tool via c: (scaffolds stubs without running)"
        mdpkg --dir /tmp/mdpkg-test --host ubuntu-test add my-curl-tool --source c --desc "custom script"
        test -f /tmp/mdpkg-test/hooks/my-curl-tool/install.sh
        test -f /tmp/mdpkg-test/hooks/my-curl-tool/remove.sh
        ! test -f /usr/local/bin/my-curl-tool

        echo "[Ubuntu] 3. Fill custom install.sh, update.sh, remove.sh, and postInstall.sh, then sync"
        cat > /tmp/mdpkg-test/hooks/my-curl-tool/install.sh <<"EOF"
#!/usr/bin/env bash
set -euo pipefail
cat > /usr/local/bin/my-curl-tool <<"INNER"
#!/usr/bin/env bash
echo "my-curl-tool v1"
INNER
chmod +x /usr/local/bin/my-curl-tool
EOF
        cat > /tmp/mdpkg-test/hooks/my-curl-tool/update.sh <<"EOF"
#!/usr/bin/env bash
set -euo pipefail
cat > /usr/local/bin/my-curl-tool <<"INNER"
#!/usr/bin/env bash
echo "my-curl-tool v2"
INNER
chmod +x /usr/local/bin/my-curl-tool
echo "updated" > "$MDPKG_DATA_DIR/custom_updated.txt"
EOF
        cat > /tmp/mdpkg-test/hooks/my-curl-tool/remove.sh <<"EOF"
#!/usr/bin/env bash
set -euo pipefail
rm -f /usr/local/bin/my-curl-tool
EOF
        cat > /tmp/mdpkg-test/hooks/my-curl-tool/postInstall.sh <<"EOF"
#!/usr/bin/env bash
set -euo pipefail
echo "postInstall ran for $MDPKG_PACKAGE" > "$MDPKG_DATA_DIR/post_install_ran.txt"
EOF
        chmod +x /tmp/mdpkg-test/hooks/my-curl-tool/*.sh

        mdpkg --dir /tmp/mdpkg-test --host ubuntu-test sync --no-confirm
        test -x /usr/local/bin/my-curl-tool
        test -f /tmp/mdpkg-test/post_install_ran.txt
        /usr/local/bin/my-curl-tool | grep -q "v1"

        echo "[Ubuntu] 4. Run mdpkg check --strict"
        mdpkg --dir /tmp/mdpkg-test --host ubuntu-test check --strict

        echo "[Ubuntu] 5. Run mdpkg update --no-confirm --ignore tree --skip-npm"
        mdpkg --dir /tmp/mdpkg-test --host ubuntu-test update --no-confirm --ignore tree --skip-npm
        test -f /tmp/mdpkg-test/custom_updated.txt
        /usr/local/bin/my-curl-tool | grep -q "v2"

        echo "[Ubuntu] 6. Disable tree and my-curl-tool and verify removal"
        mdpkg --dir /tmp/mdpkg-test --host ubuntu-test disable tree --no-confirm
        mdpkg --dir /tmp/mdpkg-test --host ubuntu-test disable my-curl-tool --no-confirm
        ! command -v tree >/dev/null
        ! test -f /usr/local/bin/my-curl-tool
        mdpkg --dir /tmp/mdpkg-test --host ubuntu-test check --strict
        echo "[Ubuntu] PASS!"
    '

echo "==> Building Manjaro / Arch test image..."
MANJARO_IMAGE="mdpkg-test-manjaro:latest"
if ! docker build -f tests/docker/Dockerfile.manjaro -t "$MANJARO_IMAGE" tests/docker; then
    echo "Warning: manjarolinux/base build failed, falling back to archlinux:latest..."
    docker build -t "$MANJARO_IMAGE" - <<'EOF'
FROM archlinux:latest
ENV MDPKG_NON_INTERACTIVE=1
RUN pacman -Sy --noconfirm
EOF
fi

echo "==> Running Manjaro / Arch (pacman/pamac) end-to-end integration test..."
docker run --rm \
    -v "$MDPKG_BIN:/usr/local/bin/mdpkg:ro" \
    "$MANJARO_IMAGE" \
    bash -ceu '
        export MDPKG_NON_INTERACTIVE=1
        export MDPKG_BACKEND=pacman

        mkdir -p /tmp/mdpkg-test
        cat > /tmp/mdpkg-test/packages.md <<EOF
# Packages

## MAIN — Core

### cli
EOF

        echo "[Manjaro/Arch] 1. Add tree via m: (repo) and verify postInstall hook"
        mkdir -p /tmp/mdpkg-test/hooks/tree
        cat > /tmp/mdpkg-test/hooks/tree/postInstall.sh <<"EOF"
#!/usr/bin/env bash
set -euo pipefail
echo "$MDPKG_PACKAGE:$MDPKG_SOURCE:$MDPKG_STAGE" > "$MDPKG_DATA_DIR/tree_post_install.txt"
EOF
        chmod +x /tmp/mdpkg-test/hooks/tree/postInstall.sh

        mdpkg --dir /tmp/mdpkg-test --host manjaro-test add tree --source m --desc "directory tree"
        command -v tree >/dev/null
        grep -q "\`tree\`" /tmp/mdpkg-test/packages.md
        grep -q -- "- \[x\] \`tree\`" /tmp/mdpkg-test/manjaro-test.md
        grep -q "tree:repo:postInstall" /tmp/mdpkg-test/tree_post_install.txt

        echo "[Manjaro/Arch] 2. Add custom package my-custom-cli via c: and sync"
        mdpkg --dir /tmp/mdpkg-test --host manjaro-test add my-custom-cli --source c --desc "custom cli"
        test -f /tmp/mdpkg-test/hooks/my-custom-cli/install.sh
        test -f /tmp/mdpkg-test/hooks/my-custom-cli/remove.sh
        ! test -f /usr/local/bin/my-custom-cli

        cat > /tmp/mdpkg-test/hooks/my-custom-cli/install.sh <<"EOF"
#!/usr/bin/env bash
set -euo pipefail
cat > /usr/local/bin/my-custom-cli <<"INNER"
#!/usr/bin/env bash
echo "my-custom-cli v1"
INNER
chmod +x /usr/local/bin/my-custom-cli
EOF
        cat > /tmp/mdpkg-test/hooks/my-custom-cli/update.sh <<"EOF"
#!/usr/bin/env bash
set -euo pipefail
cat > /usr/local/bin/my-custom-cli <<"INNER"
#!/usr/bin/env bash
echo "my-custom-cli v2"
INNER
chmod +x /usr/local/bin/my-custom-cli
echo "updated" > "$MDPKG_DATA_DIR/custom_updated.txt"
EOF
        cat > /tmp/mdpkg-test/hooks/my-custom-cli/remove.sh <<"EOF"
#!/usr/bin/env bash
set -euo pipefail
rm -f /usr/local/bin/my-custom-cli
EOF
        cat > /tmp/mdpkg-test/hooks/my-custom-cli/postInstall.sh <<"EOF"
#!/usr/bin/env bash
set -euo pipefail
echo "postInstall:$MDPKG_PACKAGE" > "$MDPKG_DATA_DIR/custom_post_install.txt"
EOF
        chmod +x /tmp/mdpkg-test/hooks/my-custom-cli/*.sh

        mdpkg --dir /tmp/mdpkg-test --host manjaro-test sync --no-confirm
        test -x /usr/local/bin/my-custom-cli
        test -f /tmp/mdpkg-test/custom_post_install.txt
        /usr/local/bin/my-custom-cli | grep -q "v1"

        echo "[Manjaro/Arch] 3. Run mdpkg check --strict"
        mdpkg --dir /tmp/mdpkg-test --host manjaro-test check --strict

        echo "[Manjaro/Arch] 4. Run mdpkg update --no-confirm --ignore filesystem --skip-npm"
        # Use a lightweight pacman wrapper for -Syu inside unprivileged container so we verify
        # exact flags (-Syu --noconfirm --ignore filesystem) and custom package update without
        # downloading ~500MB of base system upgrades or hitting read-only /etc/hosts container mounts.
        REAL_PACMAN="$(command -v pacman)"
        cat > /usr/local/bin/pacman <<EOF
#!/usr/bin/env bash
if [[ "\$*" == *"-Syu"* ]]; then
    echo "\$*" > /tmp/mdpkg-test/pacman_syu_args.txt
    exit 0
fi
exec "$REAL_PACMAN" "\$@"
EOF
        chmod +x /usr/local/bin/pacman
        export PATH="/usr/local/bin:$PATH"

        mdpkg --dir /tmp/mdpkg-test --host manjaro-test update --no-confirm --ignore filesystem --skip-npm
        grep -q -- "--ignore filesystem" /tmp/mdpkg-test/pacman_syu_args.txt
        test -f /tmp/mdpkg-test/custom_updated.txt
        /usr/local/bin/my-custom-cli | grep -q "v2"
        rm -f /usr/local/bin/pacman

        echo "[Manjaro/Arch] 5. Disable tree and my-custom-cli and verify removal"
        mdpkg --dir /tmp/mdpkg-test --host manjaro-test disable tree --no-confirm
        mdpkg --dir /tmp/mdpkg-test --host manjaro-test disable my-custom-cli --no-confirm
        ! command -v tree >/dev/null
        ! test -f /usr/local/bin/my-custom-cli
        mdpkg --dir /tmp/mdpkg-test --host manjaro-test check --strict
        echo "[Manjaro/Arch] PASS!"
    '

echo "==> All Docker end-to-end tests passed!"

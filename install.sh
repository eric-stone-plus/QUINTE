#!/bin/sh
set -eu

REPOSITORY="eric-stone-plus/QUINTE"
VERSION="${QUINTE_VERSION:-latest}"
INSTALL_DIR="${QUINTE_INSTALL_DIR:-${HOME}/.local/bin}"

case "$(uname -s)" in
  Darwin) os="apple-darwin" ;;
  Linux) os="unknown-linux-gnu" ;;
  *)
    echo "quinte: unsupported operating system: $(uname -s)" >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *)
    echo "quinte: unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

asset="quinte-${arch}-${os}.tar.gz"
if [ "$VERSION" = "latest" ]; then
  base_url="https://github.com/${REPOSITORY}/releases/latest/download"
else
  base_url="https://github.com/${REPOSITORY}/releases/download/${VERSION}"
fi

tmp_dir="$(mktemp -d 2>/dev/null || mktemp -d -t quinte-install)"
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM

echo "quinte: downloading ${asset}"
curl -fL --retry 3 --proto '=https' --tlsv1.2 \
  "${base_url}/${asset}" -o "${tmp_dir}/${asset}"
curl -fL --retry 3 --proto '=https' --tlsv1.2 \
  "${base_url}/SHA256SUMS" -o "${tmp_dir}/SHA256SUMS"

expected="$(awk -v name="$asset" '$2 == name {print $1}' "${tmp_dir}/SHA256SUMS")"
if [ -z "$expected" ]; then
  echo "quinte: ${asset} is missing from SHA256SUMS" >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "${tmp_dir}/${asset}" | awk '{print $1}')"
else
  actual="$(shasum -a 256 "${tmp_dir}/${asset}" | awk '{print $1}')"
fi
if [ "$actual" != "$expected" ]; then
  echo "quinte: checksum verification failed" >&2
  exit 1
fi

tar -xzf "${tmp_dir}/${asset}" -C "$tmp_dir"
mkdir -p "$INSTALL_DIR"
install -m 0755 "${tmp_dir}/quinte" "${INSTALL_DIR}/quinte"

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *) echo "quinte: add ${INSTALL_DIR} to PATH" ;;
esac

echo "quinte: installed ${INSTALL_DIR}/quinte"
if [ ! -f "${QUINTE_HOME:-${HOME}/.quinte}/policy.json" ]; then
  "${INSTALL_DIR}/quinte" init
fi
echo "quinte: run 'quinte doctor' to verify the fixed agent environment"

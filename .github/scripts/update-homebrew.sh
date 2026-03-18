#!/usr/bin/env bash
set -euo pipefail

cat > homebrew-mmdr/Formula/mmdr.rb <<EOF
class Mmdr < Formula
  desc "Fast Mermaid diagram renderer in pure Rust - 23 diagram types, 100-1400x faster than mermaid-cli"
  homepage "https://github.com/1jehuang/mermaid-rs-renderer"
  version "${VERSION}"
  license "MIT"

  on_macos do
    on_intel do
      url "https://github.com/1jehuang/mermaid-rs-renderer/releases/download/v${VERSION}/mmdr-x86_64-apple-darwin.tar.gz"
      sha256 "${SHA_MACOS_INTEL}"
    end
    on_arm do
      url "https://github.com/1jehuang/mermaid-rs-renderer/releases/download/v${VERSION}/mmdr-aarch64-apple-darwin.tar.gz"
      sha256 "${SHA_MACOS_ARM}"
    end
  end

  on_linux do
    url "https://github.com/1jehuang/mermaid-rs-renderer/releases/download/v${VERSION}/mmdr-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "${SHA_LINUX}"
  end

  def install
    bin.install "mmdr"
  end

  test do
    input = "flowchart LR\nA-->B\n"
    pipe_output("#{bin}/mmdr -e svg -o test.svg", input)
    assert_predicate testpath/"test.svg", :exist?
  end
end
EOF

echo "Updated homebrew formula to v${VERSION}"

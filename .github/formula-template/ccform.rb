# frozen_string_literal: true

class Ccform < Formula
  desc "Terraform-style declarative manager for Claude Code settings via a Lua DSL"
  homepage "https://github.com/fohte/ccform"
  version "VERSION_PLACEHOLDER"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/fohte/ccform/releases/download/v#{version}/ccform-aarch64-apple-darwin.tar.gz"
      sha256 "SHA256_MACOS_ARM64_PLACEHOLDER"
    end
    on_intel do
      odie "ccform is not available for macOS Intel (x86_64). Only Apple Silicon (arm64) is supported."
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/fohte/ccform/releases/download/v#{version}/ccform-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "SHA256_LINUX_ARM64_PLACEHOLDER"
    end
    on_intel do
      url "https://github.com/fohte/ccform/releases/download/v#{version}/ccform-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "SHA256_LINUX_X86_64_PLACEHOLDER"
    end
  end

  def install
    bin.install "ccform"
  end

  test do
    assert_match "ccform #{version}", shell_output("#{bin}/ccform --version")
  end
end

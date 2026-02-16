# typed: false
# frozen_string_literal: true

# QuectoClaw Homebrew formula
# Install: brew install mohammad-albarham/tap/quectoclaw
class Quectoclaw < Formula
  desc "Ultra-efficient AI coding assistant built in Rust"
  homepage "https://github.com/mohammad-albarham/QuectoClaw"
  version "0.1.0"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.intel?
      url "https://github.com/mohammad-albarham/QuectoClaw/releases/download/v#{version}/quectoclaw-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_MACOS_X86"
    elsif Hardware::CPU.arm?
      url "https://github.com/mohammad-albarham/QuectoClaw/releases/download/v#{version}/quectoclaw-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_MACOS_ARM"
    end
  end

  on_linux do
    url "https://github.com/mohammad-albarham/QuectoClaw/releases/download/v#{version}/quectoclaw-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "PLACEHOLDER_SHA256_LINUX"
  end

  def install
    bin.install "quectoclaw"
  end

  test do
    assert_match "quectoclaw #{version}", shell_output("#{bin}/quectoclaw version")
  end
end

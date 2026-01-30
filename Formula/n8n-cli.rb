class N8nCli < Formula
  desc "n8n CLI"
  homepage "https://github.com/radjathaher/n8n-cli"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/radjathaher/n8n-cli/releases/download/v0.1.0/n8n-cli-0.1.0-darwin-aarch64.tar.gz"
      sha256 "REPLACE_SHA"
    else
      odie "n8n-cli is only packaged for macOS arm64"
    end
  end

  def install
    bin.install "n8n"
  end
end

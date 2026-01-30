class N8nCli < Formula
  desc "n8n CLI"
  homepage "https://github.com/radjathaher/n8n-cli"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/radjathaher/n8n-cli/releases/download/v0.1.0/n8n-cli-0.1.0-darwin-aarch64.tar.gz"
      sha256 "15db0d237722b5bfaed971a8faf9ad3e31236b7cdf94282b6f5ba47e56731ec9"
    else
      odie "n8n-cli is only packaged for macOS arm64"
    end
  end

  def install
    bin.install "n8n"
  end
end

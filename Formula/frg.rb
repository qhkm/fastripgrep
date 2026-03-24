class Frg < Formula
  desc "Fast regex search with sparse n-gram indexing — faster than ripgrep"
  homepage "https://github.com/qhkm/fastripgrep"
  version "0.2.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/qhkm/fastripgrep/releases/download/v#{version}/frg-v#{version}-aarch64-apple-darwin.tar.gz"
    else
      url "https://github.com/qhkm/fastripgrep/releases/download/v#{version}/frg-v#{version}-x86_64-apple-darwin.tar.gz"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/qhkm/fastripgrep/releases/download/v#{version}/frg-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
    else
      url "https://github.com/qhkm/fastripgrep/releases/download/v#{version}/frg-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
    end
  end

  def install
    bin.install "frg"
  end

  test do
    assert_match "frg", shell_output("#{bin}/frg --version")
  end
end

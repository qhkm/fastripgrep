class Frg < Formula
  desc "Fast regex search with sparse n-gram indexing — faster than ripgrep"
  homepage "https://github.com/qhkm/fastripgrep"
  version "0.2.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/qhkm/fastripgrep/releases/download/v#{version}/frg-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "d4d79d163bcbfcdbdb3425386ca785f177603f06769e2f1b7dce696b0a6f6e0e"
    else
      url "https://github.com/qhkm/fastripgrep/releases/download/v#{version}/frg-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "533347bf4a525e975596fdce2aeb45d2482ab8ccb5871dd708524bce77c77321"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/qhkm/fastripgrep/releases/download/v#{version}/frg-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "52802802605d99bd1edd76131b8bdd3e79bde5ee6d3be307bf919ba77c737604"
    else
      url "https://github.com/qhkm/fastripgrep/releases/download/v#{version}/frg-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "3b96935f13427fdf9ee3b4eb5be272d21dd7006bb513ab0276c00bdabc7f996d"
    end
  end

  def install
    bin.install "frg"
  end

  test do
    assert_match "frg", shell_output("#{bin}/frg --version")
  end
end

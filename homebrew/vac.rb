class Vac < Formula
  desc "VAC — Vastar Agentic CLI"
  homepage "https://vastar.id/products/vac"
  url "https://github.com/IbrohimMid/VAC/archive/v0.1.0.tar.gz"
  sha256 "SHA256"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    system "#{bin}/vac", "--version"
  end
end

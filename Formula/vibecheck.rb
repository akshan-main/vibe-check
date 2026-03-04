class Vibecheck < Formula
  desc "Reality checks for vibe coders. One question per diff."
  homepage "https://github.com/akshan-main/vibe-check"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/akshan-main/vibe-check/releases/latest/download/vibecheck-darwin-arm64"
      sha256 "PLACEHOLDER"

      def install
        bin.install "vibecheck-darwin-arm64" => "vibecheck"
      end
    end

    on_intel do
      url "https://github.com/akshan-main/vibe-check/releases/latest/download/vibecheck-darwin-x86_64"
      sha256 "PLACEHOLDER"

      def install
        bin.install "vibecheck-darwin-x86_64" => "vibecheck"
      end
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/akshan-main/vibe-check/releases/latest/download/vibecheck-linux-x86_64"
      sha256 "PLACEHOLDER"

      def install
        bin.install "vibecheck-linux-x86_64" => "vibecheck"
      end
    end
  end

  test do
    assert_match "vibecheck", shell_output("#{bin}/vibecheck --help 2>&1", 0)
  end
end

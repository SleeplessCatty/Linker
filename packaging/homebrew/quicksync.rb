class Quicksync < Formula
  desc "Lightweight iCloud-backed selective sync for macOS"
  homepage "https://github.com/SleeplessCatty/QuickSync"
  url "https://github.com/SleeplessCatty/QuickSync/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "28db5d9e98ce5d3ae49bfef3a5f5599e99f6d167702ca8d662d0fd7767145014"
  head "https://github.com/SleeplessCatty/QuickSync.git", branch: "main"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install",
           "--locked",
           "--path", "crates/qsync-cli",
           "--root", prefix
    system "cargo", "install",
           "--locked",
           "--path", "crates/qsync-daemon",
           "--root", prefix

    (prefix/"packaging/launchagent").install "packaging/launchagent/com.quicksync.qsd.plist.in"
  end

  def caveats
    <<~EOS
      To install the background daemon, create a LaunchAgent from:
        #{prefix}/packaging/launchagent/com.quicksync.qsd.plist.in

      The daemon should run:
        #{bin}/qsd

      Logs and state are stored under:
        ~/Library/Application Support/QuickSync
    EOS
  end

  test do
    system "#{bin}/qs", "--help"
    system "#{bin}/qsd", "--help"
  end
end

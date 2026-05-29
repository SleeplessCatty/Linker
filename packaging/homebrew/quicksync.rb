class Quicksync < Formula
  desc "Lightweight iCloud-backed selective sync for macOS"
  homepage "https://github.com/SleeplessCatty/QuickSync"
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

    (prefix/"packaging/launchagent").install "packaging/launchagent/com.quicksync.qsyncd.plist.in"
  end

  def caveats
    <<~EOS
      To install the background daemon, create a LaunchAgent from:
        #{prefix}/packaging/launchagent/com.quicksync.qsyncd.plist.in

      The daemon should run:
        #{bin}/qsyncd

      Logs and state are stored under:
        ~/Library/Application Support/QuickSync
    EOS
  end

  test do
    system "#{bin}/qsync", "--help"
    system "#{bin}/qsyncd", "--help"
  end
end

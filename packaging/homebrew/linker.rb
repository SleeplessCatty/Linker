class Linker < Formula
  desc "Lightweight iCloud-backed selective sync for macOS"
  homepage "https://github.com/SleeplessCatty/Linker"
  head "https://github.com/SleeplessCatty/Linker.git", branch: "main"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install",
           "--locked",
           "--path", "crates/linker-cli",
           "--root", prefix
    system "cargo", "install",
           "--locked",
           "--path", "crates/linker-daemon",
           "--root", prefix

    (prefix/"packaging/launchagent").install "packaging/launchagent/com.linker.linkerd.plist.in"
  end

  def caveats
    <<~EOS
      To install the background daemon, create a LaunchAgent from:
        #{prefix}/packaging/launchagent/com.linker.linkerd.plist.in

      The daemon should run:
        #{bin}/linkerd

      Logs and state are stored under:
        ~/Library/Application Support/Linker
    EOS
  end

  test do
    system "#{bin}/linker", "--help"
    system "#{bin}/linkerd", "--help"
  end
end
